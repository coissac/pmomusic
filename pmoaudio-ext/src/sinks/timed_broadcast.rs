//! Broadcast channel avec TTL et propagation de TopZero.
//! Inspiré de `tokio::sync::broadcast` mais ajoute :
//! - Capacité bornée avec blocage des producteurs quand aucun slot n’est libre.
//! - Expiration automatique des messages (TTL) pour libérer les slots.
//! - Propagation d’un compteur `epoch` incrémenté sur chaque TopZeroSync.

use std::{
    collections::VecDeque,
    fmt,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};

use tokio::sync::Notify;
use tracing::warn;

/// Paquet diffusé contenant la charge utile + méta timing.
#[derive(Clone)]
pub struct TimedPacket<T> {
    /// Charge utile diffusée aux clients.
    pub payload: T,
    /// Timestamp audio relatif (en secondes) pour pacing côté client.
    pub audio_timestamp: f64,
    /// Compteur incrémenté lorsqu'un TopZeroSync est reçu.
    pub epoch: u64,
}

impl<T> fmt::Debug for TimedPacket<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TimedPacket")
            .field("audio_timestamp", &self.audio_timestamp)
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

/// Erreur remontée par `Receiver::try_recv`.
#[derive(Debug)]
pub enum TryRecvError {
    Empty,
    Lagged(u64),
    Closed,
}

/// Erreur remontée par `Receiver::recv`.
#[derive(Debug)]
pub enum RecvError {
    Lagged(u64),
    Closed,
}

/// Erreur remontée par `Sender::send`.
#[derive(Debug)]
pub struct SendError<T>(pub T);

struct Entry<T> {
    seq: u64,
    expires_at: Instant,
    payload: T,
    audio_timestamp: f64,
    epoch: u64,
}

struct State<T> {
    buffer: VecDeque<Entry<T>>,
    head_seq: u64,
    next_seq: u64,
    closed: bool,
    epoch: u64,
    epoch_start: Instant,
    cursors: Vec<Weak<ReceiverCursor>>,
}

impl<T> State<T> {
    fn new(capacity: usize, epoch_start: Instant) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            head_seq: 0,
            next_seq: 0,
            closed: false,
            epoch: 0,
            epoch_start,
            cursors: Vec::new(),
        }
    }

    fn purge_expired(&mut self, now: Instant) -> bool {
        let mut purged = 0u64;
        while let Some(entry) = self.buffer.front() {
            if entry.expires_at <= now {
                self.buffer.pop_front();
                self.head_seq += 1;
                purged += 1;
            } else {
                break;
            }
        }
        if purged > 0 {
            warn!(
                "TimedBroadcast: purged {} expired packet(s) (head_seq={})",
                purged,
                self.head_seq
            );
            return true;
        }
        false
    }

    fn prune_consumed(&mut self) -> bool {
        let mut min_next = self.next_seq;
        let mut has_cursor = false;
        self.cursors.retain(|weak| {
            if let Some(cursor) = weak.upgrade() {
                let pos = cursor.next_seq.load(Ordering::SeqCst);
                if pos < min_next {
                    min_next = pos;
                }
                has_cursor = true;
                true
            } else {
                false
            }
        });

        if !has_cursor {
            return false;
        }

        let removable = min_next.saturating_sub(self.head_seq) as usize;
        if removable == 0 {
            return false;
        }

        for _ in 0..removable {
            if self.buffer.pop_front().is_some() {
                self.head_seq += 1;
            }
        }
        true
    }
}

struct Inner<T> {
    state: Mutex<State<T>>,
    data_notify: Notify,
    space_notify: Notify,
    capacity: usize,
    sender_count: AtomicUsize,
    receiver_count: AtomicUsize,
    is_closed: AtomicBool,
}

impl<T> Inner<T> {
    fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(State::new(capacity, Instant::now())),
            data_notify: Notify::new(),
            space_notify: Notify::new(),
            capacity,
            sender_count: AtomicUsize::new(1),
            receiver_count: AtomicUsize::new(0),
            is_closed: AtomicBool::new(false),
        }
    }

    fn close(&self) {
        if !self
            .is_closed
            .swap(true, Ordering::SeqCst)
        {
            if let Ok(mut state) = self.state.lock() {
                state.closed = true;
            }
            self.data_notify.notify_waiters();
            self.space_notify.notify_waiters();
        }
    }
}

/// Créé un channel broadcast temporisé.
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    assert!(capacity > 0, "capacity must be > 0");
    let inner = Arc::new(Inner::new(capacity));
    let next_seq = {
        let state = inner
            .state
            .lock()
            .expect("timed broadcast mutex poisoned");
        state.next_seq
    };
    let sender = Sender {
        inner: inner.clone(),
    };
    let cursor = Arc::new(ReceiverCursor {
        next_seq: AtomicU64::new(next_seq),
    });
    {
        let mut state = inner
            .state
            .lock()
            .expect("timed broadcast mutex poisoned");
        state.cursors.push(Arc::downgrade(&cursor));
    }
    inner.receiver_count.store(1, Ordering::SeqCst);
    let receiver = Receiver {
        inner,
        next_seq,
        cursor,
    };
    (sender, receiver)
}

/// Sender côté producteur.
pub struct Sender<T> {
    inner: Arc<Inner<T>>,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.inner.sender_count.fetch_add(1, Ordering::SeqCst);
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Sender<T> {
    /// Diffuse un paquet. Bloque si la capacité est atteinte avec des paquets non périmés.
    pub async fn send(&self, payload: T, audio_timestamp: f64) -> Result<usize, SendError<T>>
    where
        T: Clone,
    {
        let mut payload = Some(payload);
        loop {
            let mut wait_deadline = None;
            {
                let now = Instant::now();
                let mut state = self
                    .inner
                    .state
                    .lock()
                    .expect("timed broadcast mutex poisoned");

                if state.closed {
                    return Err(SendError(payload.expect("payload already consumed")));
                }

                if state.purge_expired(now) {
                    self.inner.space_notify.notify_waiters();
                }

                if state.prune_consumed() {
                    self.inner.space_notify.notify_waiters();
                }

                if state.buffer.len() < self.inner.capacity {
                    let audio_offset =
                        Duration::from_secs_f64(audio_timestamp.max(0.0));
                    let expires_at = state.epoch_start + audio_offset;
                    let entry = Entry {
                        seq: state.next_seq,
                        expires_at,
                        payload: payload
                            .take()
                            .expect("payload already consumed"),
                        audio_timestamp,
                        epoch: state.epoch,
                    };
                    state.next_seq += 1;
                    state.buffer.push_back(entry);
                    let receivers = self.inner.receiver_count.load(Ordering::SeqCst);
                    drop(state);
                    self.inner.data_notify.notify_waiters();
                    return Ok(receivers);
                }

                wait_deadline = state.buffer.front().map(|entry| entry.expires_at);
            }

            if let Some(deadline) = wait_deadline {
                let deadline = tokio::time::Instant::from_std(deadline);
                tokio::select! {
                    _ = self.inner.space_notify.notified() => {},
                    _ = tokio::time::sleep_until(deadline) => {},
                }
            } else {
                self.inner.space_notify.notified().await;
            }
        }
    }

    /// Crée un nouveau receiver abonné au flux.
    pub fn subscribe(&self) -> Receiver<T> {
        let mut state = self
            .inner
            .state
            .lock()
            .expect("timed broadcast mutex poisoned");
        let next_seq = state.next_seq;
        let cursor = Arc::new(ReceiverCursor {
            next_seq: AtomicU64::new(next_seq),
        });
        state.cursors.push(Arc::downgrade(&cursor));
        state.prune_consumed();
        drop(state);

        self.inner.receiver_count.fetch_add(1, Ordering::SeqCst);

        Receiver {
            inner: self.inner.clone(),
            next_seq,
            cursor,
        }
    }

    /// Marque un TopZero : incrémente l'epoch pour les paquets suivants.
    pub fn mark_top_zero(&self) {
        let mut state = self
            .inner
            .state
            .lock()
            .expect("timed broadcast mutex poisoned");
        state.epoch = state.epoch.wrapping_add(1);
        state.epoch_start = Instant::now();
        if !state.buffer.is_empty() {
            state.head_seq = state.next_seq;
            state.buffer.clear();
            self.inner.space_notify.notify_waiters();
        }
    }

    /// Nombre actuel de receivers abonnés.
    pub fn receiver_count(&self) -> usize {
        self.inner.receiver_count.load(Ordering::SeqCst)
    }

    /// Ferme explicitement le channel.
    pub fn close(&self) {
        self.inner.close();
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if self.inner.sender_count.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.inner.close();
        }
    }
}

/// Receiver côté consommateur.
pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
    next_seq: u64,
    cursor: Arc<ReceiverCursor>,
}

struct ReceiverCursor {
    next_seq: AtomicU64,
}

impl<T> Receiver<T>
where
    T: Clone,
{
    fn poll_entry(&mut self) -> Result<TimedPacket<T>, TryRecvError> {
        let mut state = self
            .inner
            .state
            .lock()
            .expect("timed broadcast mutex poisoned");

        if state.closed && state.buffer.is_empty() {
            return Err(TryRecvError::Closed);
        }

        let now = Instant::now();
        if state.purge_expired(now) {
            self.inner.space_notify.notify_waiters();
        }

        if self.next_seq < state.head_seq {
            let skipped = state.head_seq - self.next_seq;
            self.next_seq = state.head_seq;
            return Err(TryRecvError::Lagged(skipped));
        }

        let offset = (self.next_seq - state.head_seq) as usize;
        if offset < state.buffer.len() {
            let entry = state
                .buffer
                .get(offset)
                .expect("invalid buffer offset");
            let packet = TimedPacket {
                payload: entry.payload.clone(),
                audio_timestamp: entry.audio_timestamp,
                epoch: entry.epoch,
            };
            self.next_seq += 1;
            self.cursor
                .next_seq
                .store(self.next_seq, Ordering::SeqCst);
            if state.prune_consumed() {
                self.inner.space_notify.notify_waiters();
            }
            return Ok(packet);
        }

        if state.closed {
            Err(TryRecvError::Closed)
        } else {
            Err(TryRecvError::Empty)
        }
    }

    /// Version synchrone utilisée dans `poll_read`.
    pub fn try_recv(&mut self) -> Result<TimedPacket<T>, TryRecvError> {
        self.poll_entry()
    }

    /// Attends qu'un paquet soit disponible.
    pub async fn recv(&mut self) -> Result<TimedPacket<T>, RecvError> {
        loop {
            match self.try_recv() {
                Ok(packet) => return Ok(packet),
                Err(TryRecvError::Empty) => {
                    self.inner.data_notify.notified().await;
                }
                Err(TryRecvError::Lagged(skipped)) => return Err(RecvError::Lagged(skipped)),
                Err(TryRecvError::Closed) => return Err(RecvError::Closed),
            }
        }
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        self.inner.receiver_count.fetch_add(1, Ordering::SeqCst);
        let cursor = Arc::new(ReceiverCursor {
            next_seq: AtomicU64::new(self.next_seq),
        });
        {
            let mut state = self
                .inner
                .state
                .lock()
                .expect("timed broadcast mutex poisoned");
            state.cursors.push(Arc::downgrade(&cursor));
        }
        Self {
            inner: self.inner.clone(),
            next_seq: self.next_seq,
            cursor,
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.cursor
            .next_seq
            .store(self.next_seq, Ordering::SeqCst);
        if let Ok(mut state) = self.inner.state.lock() {
            if state.prune_consumed() {
                self.inner.space_notify.notify_waiters();
            }
        }
        if self.inner.receiver_count.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.inner.space_notify.notify_waiters();
        }
    }
}
