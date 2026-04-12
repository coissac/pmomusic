use crate::errors::ControlPointError;
use crate::queue::{
    EnqueueMode, InternalQueue, OpenHomeQueue, QueueBackend, QueueFromRendererInfo,
};
use crate::{PlaybackItem, QueueSnapshot, RendererInfo};
use std::sync::{
    atomic::{AtomicBool, Ordering::SeqCst},
    Arc, Mutex,
};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncScheduleOutcome {
    Scheduled,
    AlreadyRunning,
}

#[derive(Debug)]
enum MusicQueueBackend {
    Internal(InternalQueue),
    OpenHome(OpenHomeQueue),
}

#[derive(Debug)]
pub struct MusicQueue {
    backend: MusicQueueBackend,
    sync_in_progress: Arc<AtomicBool>,
    sync_pending: Arc<AtomicBool>,
    sync_cancel_token: Arc<AtomicBool>,
}

impl MusicQueue {
    /// Creates a queue appropriate for the given renderer.
    /// This is the factory method used by QueueFromRendererInfo trait.
    pub fn from_renderer_info(info: &RendererInfo) -> Result<MusicQueue, ControlPointError> {
        let backend = if info.capabilities().has_oh_playlist() {
            MusicQueueBackend::OpenHome(OpenHomeQueue::from_renderer_info(info)?)
        } else {
            MusicQueueBackend::Internal(InternalQueue::from_renderer_info(info)?)
        };

        Ok(MusicQueue {
            backend,
            sync_in_progress: Arc::new(AtomicBool::new(false)),
            sync_pending: Arc::new(AtomicBool::new(false)),
            sync_cancel_token: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Creates a MusicQueue from an InternalQueue backend.
    pub fn from_internal(queue: InternalQueue) -> MusicQueue {
        MusicQueue {
            backend: MusicQueueBackend::Internal(queue),
            sync_in_progress: Arc::new(AtomicBool::new(false)),
            sync_pending: Arc::new(AtomicBool::new(false)),
            sync_cancel_token: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Creates a MusicQueue from an OpenHomeQueue backend.
    pub fn from_openhome(queue: OpenHomeQueue) -> MusicQueue {
        MusicQueue {
            backend: MusicQueueBackend::OpenHome(queue),
            sync_in_progress: Arc::new(AtomicBool::new(false)),
            sync_pending: Arc::new(AtomicBool::new(false)),
            sync_cancel_token: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns true if this is an OpenHome backend.
    pub fn is_openhome(&self) -> bool {
        matches!(self.backend, MusicQueueBackend::OpenHome(_))
    }

    /// Returns a reference to the OpenHome queue if this is an OpenHome backend.
    pub fn as_openhome(&self) -> Option<&OpenHomeQueue> {
        match &self.backend {
            MusicQueueBackend::OpenHome(q) => Some(q),
            MusicQueueBackend::Internal(_) => None,
        }
    }

    /// Returns a mutable reference to the OpenHome queue if this is an OpenHome backend.
    pub fn as_openhome_mut(&mut self) -> Option<&mut OpenHomeQueue> {
        match &mut self.backend {
            MusicQueueBackend::OpenHome(q) => Some(q),
            MusicQueueBackend::Internal(_) => None,
        }
    }

    pub fn len(&self) -> Result<usize, ControlPointError> {
        match &self.backend {
            MusicQueueBackend::Internal(q) => q.len(),
            MusicQueueBackend::OpenHome(q) => q.len(),
        }
    }

    pub fn schedule_sync(
        queue_arc: &Arc<Mutex<MusicQueue>>,
        renderer_id: &str,
        items: Vec<PlaybackItem>,
        pending_items_fn: Box<
            dyn Fn() -> Result<Vec<PlaybackItem>, ControlPointError> + Send + 'static,
        >,
        on_ready: Option<Box<dyn FnOnce() + Send + 'static>>,
        on_complete: Box<dyn Fn(usize) + Send + 'static>,
    ) -> SyncScheduleOutcome {
        let (sync_in_progress, sync_pending, sync_cancel_token) = {
            let q = queue_arc.lock().expect("MusicQueue mutex poisoned");
            (
                Arc::clone(&q.sync_in_progress),
                Arc::clone(&q.sync_pending),
                Arc::clone(&q.sync_cancel_token),
            )
        };

        if sync_in_progress.swap(true, SeqCst) {
            sync_cancel_token.store(true, SeqCst);
            sync_pending.store(true, SeqCst);
            return SyncScheduleOutcome::AlreadyRunning;
        }

        sync_cancel_token.store(false, SeqCst);
        sync_pending.store(false, SeqCst);

        let queue_arc = Arc::clone(queue_arc);
        let thread_name = format!("queue-sync-{}", renderer_id);

        thread::Builder::new()
            .name(thread_name)
            .spawn(move || {
                // Protocol for the three AtomicBools:
                //   sync_in_progress : set to true before spawn, cleared on Drop via Guard.
                //   sync_pending     : set to true by a concurrent caller that arrives while
                //                      a sync is already running.  The worker re-fetches items
                //                      and loops when it detects this flag on exit.
                //   sync_cancel_token: set to true when a new sync request interrupts an
                //                      in-progress one.  Passed into QueueBackend::sync_queue
                //                      so it can abort early.
                struct Guard(Arc<AtomicBool>);
                impl Drop for Guard {
                    fn drop(&mut self) {
                        self.0.store(false, SeqCst);
                    }
                }
                let _guard = Guard(Arc::clone(&sync_in_progress));
                // Error policy: sync errors are logged by sync_worker_loop (warn level) and
                // the thread exits normally.  No restart — a new sync can be scheduled via
                // schedule_sync.  The Guard Drop clears sync_in_progress unconditionally,
                // even on panic, keeping the AtomicBool protocol consistent.
                tracing::debug!(thread = %std::thread::current().name().unwrap_or("?"), "queue-sync thread started");
                Self::sync_worker_loop(
                    queue_arc,
                    items,
                    pending_items_fn,
                    on_ready,
                    on_complete,
                    sync_pending,
                    sync_cancel_token,
                );
                tracing::debug!(thread = %std::thread::current().name().unwrap_or("?"), "queue-sync thread done");
            })
            .expect("Failed to spawn queue-sync thread");

        SyncScheduleOutcome::Scheduled
    }

    /// Inner loop executed by the sync worker thread.
    ///
    /// Runs at least once with `initial_items`.  If a new sync request arrives while the
    /// loop is running (`sync_pending` becomes true), it re-fetches items via
    /// `pending_items_fn` and iterates again, allowing the latest playlist state to win.
    fn sync_worker_loop(
        queue_arc: Arc<Mutex<MusicQueue>>,
        initial_items: Vec<PlaybackItem>,
        pending_items_fn: Box<dyn Fn() -> Result<Vec<PlaybackItem>, ControlPointError> + Send>,
        initial_on_ready: Option<Box<dyn FnOnce() + Send>>,
        on_complete: Box<dyn Fn(usize) + Send>,
        sync_pending: Arc<AtomicBool>,
        sync_cancel_token: Arc<AtomicBool>,
    ) {
        let mut current_items = initial_items;
        let mut current_on_ready = Some(initial_on_ready);
        let mut on_complete = Some(on_complete);

        loop {
            sync_pending.store(false, SeqCst);
            sync_cancel_token.store(false, SeqCst);

            // Extract the real on_ready BEFORE locking the queue.
            // on_ready may call play_from_queue() which re-locks the queue,
            // so we must NOT call it while holding queue_arc.
            let real_on_ready = current_on_ready.take().flatten();
            let on_ready_triggered = Arc::new(AtomicBool::new(false));
            let proxy_on_ready: Option<Box<dyn FnOnce() + Send + 'static>> =
                real_on_ready.as_ref().map(|_| {
                    let flag = Arc::clone(&on_ready_triggered);
                    Box::new(move || {
                        flag.store(true, SeqCst);
                    }) as Box<dyn FnOnce() + Send + 'static>
                });

            tracing::debug!(
                thread = %std::thread::current().name().unwrap_or("?"),
                items = current_items.len(),
                has_on_ready = real_on_ready.is_some(),
                "queue-sync: calling sync_queue"
            );

            let result = {
                let mut q = queue_arc.lock().expect("MusicQueue mutex poisoned");
                <MusicQueue as QueueBackend>::sync_queue(
                    &mut q,
                    current_items,
                    &sync_cancel_token,
                    proxy_on_ready,
                )
            };
            // Queue lock is released here.
            // Now safe to call on_ready (which may re-lock the queue).
            let carry_on_ready = if on_ready_triggered.load(SeqCst) {
                tracing::debug!(
                    thread = %std::thread::current().name().unwrap_or("?"),
                    "queue-sync: on_ready triggered, calling callback"
                );
                if let Some(f) = real_on_ready {
                    f();
                }
                None
            } else {
                if real_on_ready.is_some() {
                    tracing::debug!(
                        thread = %std::thread::current().name().unwrap_or("?"),
                        "queue-sync: on_ready not triggered, carrying to next attempt"
                    );
                }
                real_on_ready
            };

            match result {
                Err(ControlPointError::SyncCancelled) => {
                    tracing::debug!(
                        thread = %std::thread::current().name().unwrap_or("?"),
                        "queue-sync: cancelled"
                    );
                }
                Err(e) => {
                    tracing::warn!("queue-sync error: {}", e);
                }
                Ok(()) => {
                    tracing::debug!(
                        thread = %std::thread::current().name().unwrap_or("?"),
                        "queue-sync: completed successfully"
                    );
                    if let Some(cb) = on_complete.take() {
                        let queue_len = queue_arc.lock().expect("MusicQueue mutex poisoned").len().unwrap_or(0);
                        cb(queue_len);
                    }
                }
            }

            if !sync_pending.load(SeqCst) {
                break;
            }

            match pending_items_fn() {
                Ok(new_items) => {
                    current_items = new_items;
                    current_on_ready = Some(carry_on_ready);
                }
                Err(e) => {
                    tracing::warn!("queue-sync pending re-fetch error: {}", e);
                    break;
                }
            }
        }
    }
}

impl QueueBackend for MusicQueue {
    // Primitives
    fn len(&self) -> Result<usize, ControlPointError> {
        match &self.backend {
            MusicQueueBackend::Internal(q) => q.len(),
            MusicQueueBackend::OpenHome(q) => q.len(),
        }
    }

    fn track_ids(&self) -> Result<Vec<u32>, ControlPointError> {
        match &self.backend {
            MusicQueueBackend::Internal(q) => q.track_ids(),
            MusicQueueBackend::OpenHome(q) => q.track_ids(),
        }
    }

    fn id_to_position(&self, id: u32) -> Result<usize, ControlPointError> {
        match &self.backend {
            MusicQueueBackend::Internal(q) => q.id_to_position(id),
            MusicQueueBackend::OpenHome(q) => q.id_to_position(id),
        }
    }

    fn position_to_id(&self, id: usize) -> Result<u32, ControlPointError> {
        match &self.backend {
            MusicQueueBackend::Internal(q) => q.position_to_id(id),
            MusicQueueBackend::OpenHome(q) => q.position_to_id(id),
        }
    }

    fn current_track(&self) -> Result<Option<u32>, ControlPointError> {
        match &self.backend {
            MusicQueueBackend::Internal(q) => q.current_track(),
            MusicQueueBackend::OpenHome(q) => q.current_track(),
        }
    }

    fn current_index(&self) -> Result<Option<usize>, ControlPointError> {
        match &self.backend {
            MusicQueueBackend::Internal(q) => q.current_index(),
            MusicQueueBackend::OpenHome(q) => q.current_index(),
        }
    }

    fn queue_snapshot(&self) -> Result<QueueSnapshot, ControlPointError> {
        match &self.backend {
            MusicQueueBackend::Internal(q) => q.queue_snapshot(),
            MusicQueueBackend::OpenHome(q) => q.queue_snapshot(),
        }
    }

    fn set_index(&mut self, index: Option<usize>) -> Result<(), ControlPointError> {
        match &mut self.backend {
            MusicQueueBackend::Internal(q) => q.set_index(index),
            MusicQueueBackend::OpenHome(q) => q.set_index(index),
        }
    }

    fn replace_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<(), ControlPointError> {
        match &mut self.backend {
            MusicQueueBackend::Internal(q) => q.replace_queue(items, current_index),
            MusicQueueBackend::OpenHome(q) => q.replace_queue(items, current_index),
        }
    }

    fn sync_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        cancel_token: &Arc<AtomicBool>,
        on_ready: Option<Box<dyn FnOnce() + Send>>,
    ) -> Result<(), ControlPointError> {
        match &mut self.backend {
            MusicQueueBackend::Internal(q) => q.sync_queue(items, cancel_token, on_ready),
            MusicQueueBackend::OpenHome(q) => q.sync_queue(items, cancel_token, on_ready),
        }
    }

    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>, ControlPointError> {
        match &self.backend {
            MusicQueueBackend::Internal(q) => q.get_item(index),
            MusicQueueBackend::OpenHome(q) => q.get_item(index),
        }
    }

    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<(), ControlPointError> {
        match &mut self.backend {
            MusicQueueBackend::Internal(q) => q.replace_item(index, item),
            MusicQueueBackend::OpenHome(q) => q.replace_item(index, item),
        }
    }

    fn enqueue_items(
        &mut self,
        items: Vec<PlaybackItem>,
        mode: EnqueueMode,
    ) -> Result<(), ControlPointError> {
        match &mut self.backend {
            MusicQueueBackend::Internal(q) => q.enqueue_items(items, mode),
            MusicQueueBackend::OpenHome(q) => q.enqueue_items(items, mode),
        }
    }

    // Optimized helpers
    fn clear_queue(&mut self) -> Result<(), ControlPointError> {
        match &mut self.backend {
            MusicQueueBackend::Internal(q) => q.clear_queue(),
            MusicQueueBackend::OpenHome(q) => q.clear_queue(),
        }
    }

    fn is_empty(&self) -> Result<bool, ControlPointError> {
        match &self.backend {
            MusicQueueBackend::Internal(q) => q.is_empty(),
            MusicQueueBackend::OpenHome(q) => q.is_empty(),
        }
    }

    fn upcoming_len(&self) -> Result<usize, ControlPointError> {
        match &self.backend {
            MusicQueueBackend::Internal(q) => q.upcoming_len(),
            MusicQueueBackend::OpenHome(q) => q.upcoming_len(),
        }
    }

    fn upcoming_items(&self) -> Result<Vec<PlaybackItem>, ControlPointError> {
        match &self.backend {
            MusicQueueBackend::Internal(q) => q.upcoming_items(),
            MusicQueueBackend::OpenHome(q) => q.upcoming_items(),
        }
    }

    fn peek_current(&mut self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        match &mut self.backend {
            MusicQueueBackend::Internal(q) => q.peek_current(),
            MusicQueueBackend::OpenHome(q) => q.peek_current(),
        }
    }

    fn dequeue_next(&mut self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        match &mut self.backend {
            MusicQueueBackend::Internal(q) => q.dequeue_next(),
            MusicQueueBackend::OpenHome(q) => q.dequeue_next(),
        }
    }

    fn append_or_init_index(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        match &mut self.backend {
            MusicQueueBackend::Internal(q) => q.append_or_init_index(items),
            MusicQueueBackend::OpenHome(q) => q.append_or_init_index(items),
        }
    }
}

impl QueueFromRendererInfo for MusicQueue {
    fn from_renderer_info(renderer: &RendererInfo) -> Result<Self, ControlPointError> {
        MusicQueue::from_renderer_info(renderer)
    }

    fn to_backend(self) -> MusicQueue {
        self
    }
}
