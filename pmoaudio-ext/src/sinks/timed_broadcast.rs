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
use tracing::{trace, info, warn};

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
    /// Aucun paquet n'est disponible pour le moment.
    Empty,
    /// Le receiver est en retard : le champ contient combien de paquets ont expiré
    /// ou ont déjà été consommés par les autres abonnés.
    ///
    /// Ce cas survient lorsque `purge_expired()` avance `head_seq` et que ce
    /// `Receiver` réclamait encore l'un des numéros supprimés. Le client doit
    /// donc ignorer les données perdues et se resynchroniser sur les paquets
    /// courants.
    Lagged(u64),
    /// Le channel est fermé et plus aucun paquet n'est disponible.
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
    last_segment_end: Option<Instant>,
    cursors: Vec<Weak<ReceiverCursor>>,
    initialized: bool,
    last_purge: Instant,
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
            last_segment_end: None,
            cursors: Vec::new(),
            initialized: false,
            last_purge: epoch_start,
        }
    }

    fn purge_expired(&mut self) -> bool {
        let now = Instant::now();

        // Throttling : purger au maximum toutes les 100ms
        if now.duration_since(self.last_purge) < Duration::from_millis(100) {
            return false;
        }
        self.last_purge = now;

        let mut purged = 0u64;
        while let Some(entry) = self.buffer.front() {
            if entry.expires_at <= now {
                let delta = now - entry.expires_at;
                // info!("TimedBroadcast: purging expired packet (epoch={},delta={})", entry.epoch, delta.as_millis());
                self.buffer.pop_front();
                self.head_seq += 1;
                purged += 1;
            } else {
                break;
            }
        }
        if purged > 0 {
            trace!(
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
        if !self.is_closed.swap(true, Ordering::SeqCst) {
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
        let state = inner.state.lock().expect("timed broadcast mutex poisoned");
        state.next_seq
    };
    let sender = Sender {
        inner: inner.clone(),
    };
    let cursor = Arc::new(ReceiverCursor {
        next_seq: AtomicU64::new(next_seq),
    });
    {
        let mut state = inner.state.lock().expect("timed broadcast mutex poisoned");
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
    ///
    /// Le TTL de chaque paquet est calculé à partir du `epoch_start` courant et du
    /// `audio_timestamp` fournis, ce qui signifie qu’un receiver en retard finira
    /// par recevoir un [`TryRecvError::Lagged`] lorsque `expires_at` est dépassé.
    pub async fn send(&self, payload: T, audio_timestamp: f64, segment_duration: f64) -> Result<usize, SendError<T>>
    where
        T: Clone,
    {
        let mut payload = Some(payload);
        loop {
            let mut wait_deadline = None;
            {
                let mut state = self
                    .inner
                    .state
                    .lock()
                    .expect("timed broadcast mutex poisoned");

                if state.closed {
                    return Err(SendError(payload.expect("payload already consumed")));
                }

                // Détecter si c'est un TopZero
                let is_top_zero = audio_timestamp == 0.0;

                // Gérer l'initialisation
                if !state.initialized {
                    if !is_top_zero {
                        warn!(
                            "TimedBroadcast: First packet should have timestamp=0.0, got {:.3}s",
                            audio_timestamp
                        );
                    }
                    let now = Instant::now();
                    state.epoch_start = now;
                    state.epoch = 0;
                    state.initialized = true;
                    info!("TimedBroadcast: initialized (epoch=0)");
                } else if is_top_zero {
                    // TopZero = nouveau segment, toujours valide après l'initialisation
                    let now = Instant::now();
                    // Continuité temporelle : nouveau segment commence après le précédent
                    state.epoch_start = state.last_segment_end.unwrap_or(now);
                    state.epoch = state.epoch.wrapping_add(1);
                    info!(
                        "TimedBroadcast: new epoch={} (continuous={})",
                        state.epoch,
                        state.last_segment_end.is_some()
                    );
                }

                // 1. Calculer l'expiration du paquet actuel
                let expires_at = state.epoch_start + Duration::from_secs_f64(audio_timestamp + segment_duration);

                // 2. Vérifier que le paquet n'est pas déjà expiré
                let now = Instant::now();
                if expires_at <= now {
                    warn!(
                        "TimedBroadcast: packet already expired (ts={:.3}s, delta={}ms)",
                        audio_timestamp,
                        now.duration_since(expires_at).as_millis()
                    );
                    // On peut décider de l'ignorer ou de continuer
                    // Pour l'instant on continue pour ne pas bloquer le flux
                }

                // 3. Purger les paquets expirés et consommés
                let consumed = state.prune_consumed();
                let expired = state.purge_expired();
                if consumed || expired {
                    self.inner.space_notify.notify_waiters();
                }

                // 4. Vérifier la capacité et insérer
                if state.buffer.len() < self.inner.capacity {
                    let entry = Entry {
                        seq: state.next_seq,
                        expires_at,
                        payload: payload.take().expect("payload already consumed"),
                        audio_timestamp,
                        epoch: state.epoch,
                    };
                    state.next_seq += 1;
                    state.buffer.push_back(entry);

                    // 5. Stocker la fin du segment SEULEMENT pour les paquets non-TopZero
                    if !is_top_zero {
                        state.last_segment_end = Some(expires_at);
                    }

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
///
/// Chaque receiver garde son propre curseur `next_seq`. Si le producteur
/// recycle un paquet via `purge_expired()` avant que ce curseur ne l’ait lu,
/// la prochaine tentative de lecture retournera [`TryRecvError::Lagged`].
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

        if state.purge_expired() {
            self.inner.space_notify.notify_waiters();
        }

        if self.next_seq < state.head_seq {
            let skipped = state.head_seq - self.next_seq;
            self.next_seq = state.head_seq;
            return Err(TryRecvError::Lagged(skipped));
        }

        let offset = (self.next_seq - state.head_seq) as usize;
        if offset < state.buffer.len() {
            let entry = state.buffer.get(offset).expect("invalid buffer offset");
            let packet = TimedPacket {
                payload: entry.payload.clone(),
                audio_timestamp: entry.audio_timestamp,
                epoch: entry.epoch,
            };
            self.next_seq += 1;
            self.cursor.next_seq.store(self.next_seq, Ordering::SeqCst);
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
    ///
    /// # Erreurs
    ///
    /// * [`TryRecvError::Lagged`] — des paquets ont expiré avant d'être consommés.
    /// * [`TryRecvError::Empty`] — la file est vide pour l'instant.
    /// * [`TryRecvError::Closed`] — plus aucun paquet n'arrivera.
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
        self.cursor.next_seq.store(self.next_seq, Ordering::SeqCst);
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
