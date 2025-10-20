// logs.rs
mod sselayer;

use pmoconfig::get_config;
pub use sselayer::SseLayer;

use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
    time::SystemTime,
};

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::Level;
use tracing_subscriber::{
    Registry,
    filter::LevelFilter,
    layer::{Filter, SubscriberExt},
    reload,
    util::SubscriberInitExt,
};
use utoipa::OpenApi;

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
    max_level: Arc<RwLock<Level>>,
    reload_handle: Arc<RwLock<reload::Handle<LevelFilter, Registry>>>,
}

impl LogState {
    pub fn new(capacity: usize, reload_handle: reload::Handle<LevelFilter, Registry>) -> Self {
        Self {
            buffer: Arc::new(RwLock::new(VecDeque::with_capacity(capacity))),
            tx: broadcast::channel(1000).0,
            max_level: Arc::new(RwLock::new(Level::TRACE)),
            reload_handle: Arc::new(RwLock::new(reload_handle)),
        }
    }

    pub fn set_max_level(&self, level: Level) {
        *self.max_level.write().unwrap() = level;

        // Convertir Level en LevelFilter
        let level_filter = level_to_levelfilter(level);

        // Recharger le filtre dynamiquement
        if let Err(e) = self.reload_handle.write().unwrap().reload(level_filter) {
            eprintln!("❌ Failed to reload log level filter: {}", e);
        } else {
            eprintln!(
                "✅ Log level filter reloaded successfully to: {:?}",
                level_filter
            );
        }
    }

    pub fn get_max_level(&self) -> Level {
        *self.max_level.read().unwrap()
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

    // Récupérer l'historique du buffer et le niveau actuel
    let history = state.dump();
    let stream_state = state.clone();
    let current_level = stream_state.get_max_level();

    let stream = async_stream::stream! {
        // 1. Envoyer d'abord tous les logs historiques filtrés par le niveau actuel
        for entry in history {
            // Filtrer par le niveau actuel du serveur
            if !is_level_allowed(&entry.level, current_level) {
                continue;
            }

            if !filter_entry(&entry, &params) {
                continue;
            }
            let json = serde_json::to_string(&entry).unwrap();
            yield Ok::<_, axum::Error>(Event::default().data(json));
        }

        // 2. Puis streamer les nouveaux logs en temps réel
        while let Ok(entry) = rx.recv().await {
            let max_level = stream_state.get_max_level();
            if !is_level_allowed(&entry.level, max_level) {
                continue;
            }
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

/// Vérifie si un niveau de log est autorisé selon le niveau maximum configuré
fn is_level_allowed(log_level: &str, max_level: Level) -> bool {
    let entry_level = match log_level.to_uppercase().as_str() {
        "ERROR" => Level::ERROR,
        "WARN" => Level::WARN,
        "INFO" => Level::INFO,
        "DEBUG" => Level::DEBUG,
        "TRACE" => Level::TRACE,
        _ => return false,
    };

    // Comparer les niveaux : un log est autorisé si son niveau est <= max_level
    // ERROR(1) <= WARN(2) <= INFO(3) <= DEBUG(4) <= TRACE(5)
    match max_level {
        Level::ERROR => matches!(entry_level, Level::ERROR),
        Level::WARN => matches!(entry_level, Level::ERROR | Level::WARN),
        Level::INFO => matches!(entry_level, Level::ERROR | Level::WARN | Level::INFO),
        Level::DEBUG => matches!(
            entry_level,
            Level::ERROR | Level::WARN | Level::INFO | Level::DEBUG
        ),
        Level::TRACE => true, // Tous les niveaux
    }
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

/// Options d'initialisation du système de logging
#[derive(Debug, Clone)]
pub struct LoggingOptions {
    /// Capacité du buffer circulaire (nombre d'entrées conservées)
    pub buffer_capacity: usize,
    /// Activer la sortie vers stderr/stdout
    pub enable_console: bool,
}

impl Default for LoggingOptions {
    fn default() -> Self {
        Self {
            buffer_capacity: 1000,
            enable_console: true,
        }
    }
}

/// Initialise le système de logging avec SSE et optionnellement la console
///
/// # Arguments
/// * `options` - Options de configuration du logging
///
/// # Retourne
/// Le `LogState` qui peut être utilisé pour ajouter les routes de logging au serveur
///
/// # Exemple
/// ```rust,no_run
/// use pmoserver::logs::{init_logging, LoggingOptions};
///
/// let log_state = init_logging(LoggingOptions {
///     buffer_capacity: 1000,
///     enable_console: true,
/// });
/// ```
pub fn init_logging() -> LogState {
    let config = get_config();
    // Créer un filtre rechargeable qui commence à TRACE

    let log_level = match config.get_log_min_level() {
        Ok(l) => match string_to_level(&l) {
            Some(lev) => level_to_levelfilter(lev),
            None => LevelFilter::TRACE,
        },
        Err(_) => LevelFilter::TRACE,
    };

    let (filter, reload_handle) = reload::Layer::new(log_level);

    let buffer_capacity = match config.get_log_cache_size() {
        Ok(c) => c,
        Err(_) => 500,
    };

    // Créer le LogState avec le handle de rechargement
    let log_state = LogState::new(buffer_capacity, reload_handle);

    // Construire le subscriber avec le filtre rechargeable AVANT le SseLayer
    // L'ordre est important : le filtre doit être appliqué en premier
    let subscriber = Registry::default()
        .with(filter)
        .with(SseLayer::new(log_state.clone()));

    let enable_console = match config.get_log_enable_console() {
        Ok(b) => b,
        Err(_) => true,
    };

    if enable_console {
        subscriber
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(true)
                    .with_level(true)
                    .with_ansi(true),
            )
            .init();
    } else {
        subscriber.init();
    }

    log_state
}

/// Request body pour la configuration du logging
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LogSetupRequest {
    pub level: String,
}

/// Response pour la configuration du logging
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LogSetupResponse {
    pub current_level: String,
    pub available_levels: Vec<String>,
}

/// Handler pour GET /api/log_setup - retourne la configuration actuelle
#[utoipa::path(
    get,
    path = "/api/log_setup",
    responses(
        (status = 200, description = "Log configuration retrieved successfully", body = LogSetupResponse)
    ),
    tag = "logs"
)]
pub async fn log_setup_get(State(state): State<LogState>) -> impl IntoResponse {
    let current = level_to_string(state.get_max_level());
    Json(LogSetupResponse {
        current_level: current,
        available_levels: vec![
            "ERROR".to_string(),
            "WARN".to_string(),
            "INFO".to_string(),
            "DEBUG".to_string(),
            "TRACE".to_string(),
        ],
    })
}

/// Handler pour POST /api/log_setup - met à jour le niveau de log
#[utoipa::path(
    post,
    path = "/api/log_setup",
    request_body = LogSetupRequest,
    responses(
        (status = 200, description = "Log level updated successfully", body = LogSetupResponse),
        (status = 400, description = "Invalid log level")
    ),
    tag = "logs"
)]
pub async fn log_setup_post(
    State(state): State<LogState>,
    Json(payload): Json<LogSetupRequest>,
) -> impl IntoResponse {
    let level = match string_to_level(&payload.level) {
        Some(l) => l,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Invalid log level. Must be one of: ERROR, WARN, INFO, DEBUG, TRACE"
                })),
            )
                .into_response();
        }
    };

    state.set_max_level(level);
    tracing::info!("Log level changed to: {}", payload.level);

    (
        StatusCode::OK,
        Json(LogSetupResponse {
            current_level: level_to_string(level),
            available_levels: vec![
                "ERROR".to_string(),
                "WARN".to_string(),
                "INFO".to_string(),
                "DEBUG".to_string(),
                "TRACE".to_string(),
            ],
        }),
    )
        .into_response()
}

fn string_to_level(s: &str) -> Option<Level> {
    match s.to_uppercase().as_str() {
        "ERROR" => Some(Level::ERROR),
        "WARN" => Some(Level::WARN),
        "INFO" => Some(Level::INFO),
        "DEBUG" => Some(Level::DEBUG),
        "TRACE" => Some(Level::TRACE),
        _ => None,
    }
}

fn level_to_string(level: Level) -> String {
    match level {
        Level::ERROR => "ERROR",
        Level::WARN => "WARN",
        Level::INFO => "INFO",
        Level::DEBUG => "DEBUG",
        Level::TRACE => "TRACE",
    }
    .to_string()
}

fn level_to_levelfilter(level: Level) -> LevelFilter {
    match level {
        Level::ERROR => LevelFilter::ERROR,
        Level::WARN => LevelFilter::WARN,
        Level::INFO => LevelFilter::INFO,
        Level::DEBUG => LevelFilter::DEBUG,
        Level::TRACE => LevelFilter::TRACE,
    }
}

/// Crée le router pour l'API de gestion des logs
pub fn create_logs_router(log_state: LogState) -> axum::Router {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/log_setup", get(log_setup_get).post(log_setup_post))
        .with_state(log_state)
}

/// API OpenAPI pour la gestion des logs
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        log_setup_get,
        log_setup_post,
    ),
    components(
        schemas(LogSetupRequest, LogSetupResponse)
    ),
    tags(
        (name = "logs", description = "Log level configuration endpoints")
    )
)]
pub struct LogsApiDoc;
