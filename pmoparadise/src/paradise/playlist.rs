//! Shared playlist structures for Radio Paradise channels.
//!
//! This module keeps track of the active queue and history for a Radio
//! Paradise channel. Each playlist entry knows how many clients still need
//! to consume it before the worker can evict it.

use super::history::{HistoryEntry, SongSnapshot};
use crate::models::Song;
use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{Notify, RwLock};

/// Metadata stored for an active track.
#[derive(Debug)]
pub struct PlaylistEntry {
    pub track_id: String,
    pub channel_id: u8,
    pub song: Arc<Song>,
    pub started_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub audio_pk: Option<String>,
    pub file_path: Option<PathBuf>,
    pending_clients: AtomicUsize,
}

impl PlaylistEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        track_id: String,
        channel_id: u8,
        song: Arc<Song>,
        started_at: DateTime<Utc>,
        duration_ms: u64,
        audio_pk: Option<String>,
        file_path: Option<PathBuf>,
        pending_clients: usize,
    ) -> Self {
        Self {
            track_id,
            channel_id,
            song,
            started_at,
            duration_ms,
            audio_pk,
            file_path,
            pending_clients: AtomicUsize::new(pending_clients),
        }
    }

    pub fn as_history_entry(&self) -> HistoryEntry {
        HistoryEntry {
            track_id: self.track_id.clone(),
            channel_id: self.channel_id,
            started_at: self.started_at,
            duration_ms: self.duration_ms,
            song: SongSnapshot::from(self.song.as_ref()),
        }
    }

    pub fn pending_clients(&self) -> usize {
        self.pending_clients.load(Ordering::SeqCst)
    }

    pub fn set_pending_clients(&self, value: usize) {
        self.pending_clients.store(value, Ordering::SeqCst);
    }

    pub fn increment_clients(&self) -> usize {
        self.pending_clients.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn decrement_clients(&self) -> usize {
        let mut current = self.pending_clients.load(Ordering::SeqCst);
        loop {
            if current == 0 {
                return 0;
            }
            match self.pending_clients.compare_exchange(
                current,
                current - 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return current - 1,
                Err(actual) => current = actual,
            }
        }
    }
}

#[derive(Default)]
struct PlaylistState {
    active: VecDeque<Arc<PlaylistEntry>>,
    history: VecDeque<HistoryEntry>,
    max_history: usize,
}

impl PlaylistState {
    fn new(max_history: usize) -> Self {
        Self {
            active: VecDeque::new(),
            history: VecDeque::new(),
            max_history,
        }
    }

    fn active_len(&self) -> usize {
        self.active.len()
    }

    fn push_active(&mut self, entry: Arc<PlaylistEntry>) {
        self.active.push_back(entry);
    }

    fn active_snapshot(&self) -> Vec<Arc<PlaylistEntry>> {
        self.active.iter().cloned().collect()
    }

    fn pop_front_if_ready(&mut self) -> Option<Arc<PlaylistEntry>> {
        if let Some(front) = self.active.front() {
            if front.pending_clients() == 0 {
                return self.active.pop_front();
            }
        }
        None
    }

    fn pop_front_matching(&mut self, track_id: &str) -> Option<Arc<PlaylistEntry>> {
        if let Some(front) = self.active.front() {
            if front.track_id == track_id && front.pending_clients() == 0 {
                return self.active.pop_front();
            }
        }
        None
    }

    fn push_history(&mut self, entry: HistoryEntry) {
        self.history.push_back(entry);
        self.trim_history();
    }

    fn recent_history(&self, limit: usize) -> Vec<HistoryEntry> {
        let total = self.history.len();
        let start = total.saturating_sub(limit);
        self.history.iter().skip(start).cloned().collect()
    }

    fn trim_history(&mut self) {
        while self.history.len() > self.max_history {
            self.history.pop_front();
        }
    }

    fn clear(&mut self) -> bool {
        let changed = !self.active.is_empty() || !self.history.is_empty();
        if changed {
            self.active.clear();
            self.history.clear();
        }
        changed
    }

    fn increment_all(&self) {
        for entry in &self.active {
            entry.increment_clients();
        }
    }
}

struct SharedPlaylistInner {
    state: RwLock<PlaylistState>,
    notify: Notify,
    update_id: AtomicU32,
    last_change: RwLock<Option<SystemTime>>,
}

#[derive(Clone)]
pub struct SharedPlaylist(Arc<SharedPlaylistInner>);

impl SharedPlaylist {
    pub fn new(max_history: usize) -> Self {
        Self(Arc::new(SharedPlaylistInner {
            state: RwLock::new(PlaylistState::new(max_history)),
            notify: Notify::new(),
            update_id: AtomicU32::new(0),
            last_change: RwLock::new(None),
        }))
    }

    async fn touch(&self) {
        self.0.update_id.fetch_add(1, Ordering::SeqCst);
        let mut last_change = self.0.last_change.write().await;
        *last_change = Some(SystemTime::now());
    }

    pub async fn push_active(&self, entry: Arc<PlaylistEntry>) {
        let mut guard = self.0.state.write().await;
        guard.push_active(entry);
        drop(guard);
        self.touch().await;
        self.0.notify.notify_waiters();
    }

    pub async fn active_len(&self) -> usize {
        let guard = self.0.state.read().await;
        guard.active_len()
    }

    pub async fn active_snapshot(&self) -> Vec<Arc<PlaylistEntry>> {
        let guard = self.0.state.read().await;
        guard.active_snapshot()
    }

    pub async fn clear(&self) {
        let mut guard = self.0.state.write().await;
        let changed = guard.clear();
        drop(guard);
        if changed {
            self.touch().await;
            self.0.notify.notify_waiters();
        }
    }

    pub async fn wait_for_track_count(&self, current_len: usize) {
        loop {
            let len = {
                let guard = self.0.state.read().await;
                guard.active_len()
            };

            if len > current_len {
                break;
            }

            self.0.notify.notified().await;
        }
    }

    pub async fn pop_front_if_ready(&self) -> Option<Arc<PlaylistEntry>> {
        let mut guard = self.0.state.write().await;
        let result = guard.pop_front_if_ready();
        drop(guard);

        if result.is_some() {
            self.touch().await;
            self.0.notify.notify_waiters();
        }

        result
    }

    pub async fn pop_front_matching(&self, track_id: &str) -> Option<Arc<PlaylistEntry>> {
        let mut guard = self.0.state.write().await;
        let result = guard.pop_front_matching(track_id);
        drop(guard);

        if result.is_some() {
            self.touch().await;
            self.0.notify.notify_waiters();
        }

        result
    }

    pub async fn push_history_entry(&self, entry: HistoryEntry) {
        let mut guard = self.0.state.write().await;
        guard.push_history(entry);
        drop(guard);
        self.touch().await;
    }

    pub async fn recent_history(&self, limit: usize) -> Vec<HistoryEntry> {
        let guard = self.0.state.read().await;
        guard.recent_history(limit)
    }

    pub async fn increment_all_pending(&self) {
        let guard = self.0.state.read().await;
        guard.increment_all();
    }

    pub fn update_id(&self) -> u32 {
        self.0.update_id.load(Ordering::SeqCst)
    }

    pub async fn last_change(&self) -> Option<SystemTime> {
        self.0.last_change.read().await.clone()
    }
}
