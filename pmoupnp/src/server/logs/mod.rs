// logs.rs
mod sselayer;

pub use sselayer::SseLayer;

use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
    time::SystemTime,
};

use axum::{
    Json,
    extract::{Query, State},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Représente une entrée de log
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: SystemTime,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// Buffer circulaire partagé
#[derive(Clone)]
pub struct LogState {
    buffer: Arc<RwLock<VecDeque<LogEntry>>>,
    tx: broadcast::Sender<LogEntry>,
}

impl LogState {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Arc::new(RwLock::new(VecDeque::with_capacity(capacity))),
            tx: broadcast::channel(1000).0,
        }
    }

    fn push(&self, entry: LogEntry) {
        let mut buf = self.buffer.write().unwrap();
        if buf.len() == buf.capacity() {
            buf.pop_front();
        }
        buf.push_back(entry.clone());
        let _ = self.tx.send(entry);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.tx.subscribe()
    }

    pub fn dump(&self) -> Vec<LogEntry> {
        self.buffer.read().unwrap().iter().cloned().collect()
    }
}

/// Query params pour /log-sse
#[derive(Debug, Deserialize)]
pub struct LogQuery {
    #[serde(default)]
    pub error: Option<bool>,
    #[serde(default)]
    pub warn: Option<bool>,
    #[serde(default)]
    pub info: Option<bool>,
    #[serde(default)]
    pub debug: Option<bool>,
    #[serde(default)]
    pub trace: Option<bool>,
    #[serde(default)]
    pub search: Option<String>,
}

/// Handler SSE
// Dans logs.rs
pub async fn log_sse(
    State(state): State<LogState>,
    Query(params): Query<LogQuery>,
) -> impl IntoResponse {
    let mut rx = state.subscribe();

    // Récupérer l'historique du buffer
    let history = state.dump();

    let stream = async_stream::stream! {
        // 1. Envoyer d'abord tous les logs historiques
        for entry in history {
            if !filter_entry(&entry, &params) {
                continue;
            }
            let json = serde_json::to_string(&entry).unwrap();
            yield Ok::<_, axum::Error>(Event::default().data(json));
        }

        // 2. Puis streamer les nouveaux logs en temps réel
        while let Ok(entry) = rx.recv().await {
            if !filter_entry(&entry, &params) {
                continue;
            }
            let json = serde_json::to_string(&entry).unwrap();
            yield Ok::<_, axum::Error>(Event::default().data(json));
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Handler REST (dump JSON du buffer)
pub async fn log_dump(State(state): State<LogState>) -> impl IntoResponse {
    Json(state.dump())
}

/// Fonction de filtrage
fn filter_entry(entry: &LogEntry, q: &LogQuery) -> bool {
    // Filtrage par niveau
    let lvl = entry.level.to_lowercase();
    let mut allowed = false;

    if let Some(true) = q.error {
        allowed |= lvl == "error";
    }
    if let Some(true) = q.warn {
        allowed |= lvl == "warn";
    }
    if let Some(true) = q.info {
        allowed |= lvl == "info";
    }
    if let Some(true) = q.debug {
        allowed |= lvl == "debug";
    }
    if let Some(true) = q.trace {
        allowed |= lvl == "trace";
    }

    // si aucun flag → tout est autorisé
    if !(q.error.unwrap_or(false)
        || q.warn.unwrap_or(false)
        || q.info.unwrap_or(false)
        || q.debug.unwrap_or(false)
        || q.trace.unwrap_or(false))
    {
        allowed = true;
    }

    // Filtrage par mot-clé
    if let Some(search) = &q.search {
        allowed &= entry.message.contains(search) || entry.target.contains(search);
    }

    allowed
}
