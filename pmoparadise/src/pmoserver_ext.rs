//! Extension pmoserver pour Radio Paradise
//!
//! Ce module fournit un trait d'extension pour ajouter facilement l'API Radio Paradise
//! à un serveur pmoserver.

use crate::paradise::{max_channel_id, ParadiseChannel, PlaylistEntry, ALL_CHANNELS};
use crate::{Block, NowPlaying, RadioParadiseClient, RadioParadiseSource};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use pmosource::api::CacheStatusInfo;
use pmosource::CacheStatus;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::error;
use utoipa::{IntoParams, OpenApi, ToSchema};

/// État partagé pour l'API Radio Paradise
#[derive(Clone)]
pub struct RadioParadiseState {
    client: Arc<RwLock<RadioParadiseClient>>,
    source: Arc<RadioParadiseSource>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ParadiseQuery {
    channel: Option<u8>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(default)]
#[into_params(parameter_in = Query)]
struct ListLimitQuery {
    /// Nombre maximum d'éléments à retourner (0 = tous)
    #[serde(default)]
    limit: Option<usize>,
}

impl RadioParadiseState {
    pub async fn new() -> anyhow::Result<Self> {
        let client = RadioParadiseClient::new()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create RadioParadise client: {}", e))?;
        #[cfg(feature = "server")]
        let source = RadioParadiseSource::from_registry_default(client.clone())
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        #[cfg(not(feature = "server"))]
        let source = {
            let base_dir = std::env::temp_dir().join("pmoparadise_api");
            let cover_dir = base_dir.join("covers");
            let audio_dir = base_dir.join("audio");
            std::fs::create_dir_all(&cover_dir)?;
            std::fs::create_dir_all(&audio_dir)?;

            let cover_cache = Arc::new(pmocovers::cache::new_cache(
                cover_dir.to_string_lossy().as_ref(),
                256,
            )?);
            let audio_cache = Arc::new(pmoaudiocache::cache::new_cache(
                audio_dir.to_string_lossy().as_ref(),
                256,
            )?);
            RadioParadiseSource::new_default(client.clone(), cover_cache, audio_cache)
        };

        Ok(Self {
            client: Arc::new(RwLock::new(client)),
            source: Arc::new(source),
        })
    }

    async fn client_for_params(
        &self,
        params: &ParadiseQuery,
    ) -> Result<RadioParadiseClient, StatusCode> {
        let base_client = {
            let client_guard = self.client.read().await;
            client_guard.clone()
        };

        let mut client = base_client;

        if let Some(channel) = params.channel {
            if channel > max_channel_id() {
                tracing::warn!("Invalid Radio Paradise channel requested: {}", channel);
                return Err(StatusCode::BAD_REQUEST);
            }
            client = client.clone_with_channel(channel);
        }

        Ok(client)
    }

    fn channel_for_id(&self, channel_id: u8) -> Result<Arc<ParadiseChannel>, StatusCode> {
        if channel_id > max_channel_id() {
            return Err(StatusCode::BAD_REQUEST);
        }
        self.source
            .channel(channel_id)
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)
    }

    pub fn source(&self) -> Arc<RadioParadiseSource> {
        self.source.clone()
    }
}

/// Information sur un canal Radio Paradise
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChannelInfo {
    /// ID du canal (0-3)
    pub id: u8,
    /// Nom du canal
    pub name: String,
    /// Description
    pub description: String,
}

impl From<&crate::paradise::ChannelDescriptor> for ChannelInfo {
    fn from(descriptor: &crate::paradise::ChannelDescriptor) -> Self {
        Self {
            id: descriptor.id,
            name: descriptor.display_name.to_string(),
            description: descriptor.description.to_string(),
        }
    }
}

/// Réponse avec informations étendues sur le morceau en cours
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NowPlayingResponse {
    /// Event ID du block actuel
    pub event: u64,
    /// Event ID du prochain block
    pub end_event: u64,
    /// URL de streaming du block
    pub stream_url: String,
    /// Durée totale du block en ms
    pub block_length_ms: u64,
    /// Index du morceau actuel
    pub current_song_index: Option<usize>,
    /// Morceau actuel
    pub current_song: Option<SongInfo>,
    /// Tous les morceaux du block
    pub songs: Vec<SongInfo>,
}

/// Information sur un morceau
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SongInfo {
    /// Index dans le block
    pub index: usize,
    /// Artiste
    pub artist: String,
    /// Titre
    pub title: String,
    /// Album
    pub album: String,
    /// Année
    pub year: Option<u32>,
    /// Temps écoulé depuis le début du block (ms)
    pub elapsed_ms: u64,
    /// Durée du morceau (ms)
    pub duration_ms: u64,
    /// URL de la pochette
    pub cover_url: Option<String>,
    /// Note (0-10)
    pub rating: Option<f32>,
}

/// Réponse pour un block
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BlockResponse {
    /// Event ID du block
    pub event: u64,
    /// Event ID du prochain block
    pub end_event: u64,
    /// URL de streaming
    pub url: String,
    /// Durée totale (ms)
    pub length_ms: u64,
    /// Morceaux du block
    pub songs: Vec<SongInfo>,
}

impl From<Block> for BlockResponse {
    fn from(block: Block) -> Self {
        let songs = block
            .songs_ordered()
            .into_iter()
            .map(|(index, song)| SongInfo {
                index,
                artist: song.artist.clone(),
                title: song.title.clone(),
                album: song.album.clone().unwrap_or_default(),
                year: song.year,
                elapsed_ms: song.elapsed,
                duration_ms: song.duration,
                cover_url: song.cover.as_ref().and_then(|c| block.cover_url(c)),
                rating: song.rating,
            })
            .collect();

        Self {
            event: block.event,
            end_event: block.end_event,
            url: block.url,
            length_ms: block.length,
            songs,
        }
    }
}

impl From<NowPlaying> for NowPlayingResponse {
    fn from(np: NowPlaying) -> Self {
        let songs: Vec<SongInfo> = np
            .block
            .songs_ordered()
            .into_iter()
            .map(|(index, song)| SongInfo {
                index,
                artist: song.artist.clone(),
                title: song.title.clone(),
                album: song.album.clone().unwrap_or_default(),
                year: song.year,
                elapsed_ms: song.elapsed,
                duration_ms: song.duration,
                cover_url: song.cover.as_ref().and_then(|c| np.block.cover_url(c)),
                rating: song.rating,
            })
            .collect();

        let current_song = np.current_song.as_ref().and_then(|song| {
            let index = np.current_song_index?;
            Some(SongInfo {
                index,
                artist: song.artist.clone(),
                title: song.title.clone(),
                album: song.album.clone().unwrap_or_default(),
                year: song.year,
                elapsed_ms: song.elapsed,
                duration_ms: song.duration,
                cover_url: song.cover.as_ref().and_then(|c| np.block.cover_url(c)),
                rating: song.rating,
            })
        });

        Self {
            event: np.block.event,
            end_event: np.block.end_event,
            stream_url: np.block.url,
            block_length_ms: np.block.length,
            current_song_index: np.current_song_index,
            current_song,
            songs,
        }
    }
}

/// GET /now-playing - Récupère le morceau en cours
#[utoipa::path(
    get,
    path = "/now-playing",
    params(
        ("channel" = Option<u8>, Query, description = "Channel ID (0-3)")
    ),
    responses(
        (status = 200, description = "Morceau en cours", body = NowPlayingResponse),
        (status = 500, description = "Erreur serveur")
    ),
    tag = "Radio Paradise"
)]
async fn get_now_playing(
    State(state): State<RadioParadiseState>,
    Query(params): Query<ParadiseQuery>,
) -> Result<Json<NowPlayingResponse>, StatusCode> {
    let client = state.client_for_params(&params).await?;
    let now_playing = client.now_playing().await.map_err(|e| {
        tracing::error!("Failed to fetch now playing from Radio Paradise: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(now_playing.into()))
}

/// GET /block/current - Récupère le block actuel
#[utoipa::path(
    get,
    path = "/block/current",
    params(
        ("channel" = Option<u8>, Query, description = "Channel ID (0-3)")
    ),
    responses(
        (status = 200, description = "Block actuel", body = BlockResponse),
        (status = 500, description = "Erreur serveur")
    ),
    tag = "Radio Paradise"
)]
async fn get_current_block(
    State(state): State<RadioParadiseState>,
    Query(params): Query<ParadiseQuery>,
) -> Result<Json<BlockResponse>, StatusCode> {
    let client = state.client_for_params(&params).await?;
    let block = client.get_block(None).await.map_err(|e| {
        tracing::error!("Failed to fetch current block from Radio Paradise: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(block.into()))
}

/// GET /block/{event_id} - Récupère un block spécifique
#[utoipa::path(
    get,
    path = "/block/{event_id}",
    params(
        ("event_id" = u64, Path, description = "Event ID du block"),
        ("channel" = Option<u8>, Query, description = "Channel ID (0-3)")
    ),
    responses(
        (status = 200, description = "Block demandé", body = BlockResponse),
        (status = 500, description = "Erreur serveur")
    ),
    tag = "Radio Paradise"
)]
async fn get_block_by_id(
    State(state): State<RadioParadiseState>,
    Path(event_id): Path<u64>,
    Query(params): Query<ParadiseQuery>,
) -> Result<Json<BlockResponse>, StatusCode> {
    let client = state.client_for_params(&params).await?;
    let block = client.get_block(Some(event_id)).await.map_err(|e| {
        tracing::error!(
            "Failed to fetch block {} from Radio Paradise: {}",
            event_id,
            e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(block.into()))
}

/// GET /channels - Liste les canaux disponibles
#[utoipa::path(
    get,
    path = "/channels",
    responses(
        (status = 200, description = "Liste des canaux", body = Vec<ChannelInfo>)
    ),
    tag = "Radio Paradise"
)]
async fn get_channels() -> Json<Vec<ChannelInfo>> {
    let channels: Vec<ChannelInfo> = ALL_CHANNELS.iter().map(Into::into).collect();
    Json(channels)
}

/// Statut opérationnel d'un canal Radio Paradise
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChannelStatusResponse {
    /// ID numérique du canal
    pub channel_id: u8,
    /// Slug du canal (main, mellow, ...)
    pub slug: String,
    /// Nom complet du canal
    pub name: String,
    /// Description
    pub description: String,
    /// Nombre de clients connectés au flux
    pub active_clients: usize,
    /// Nombre de morceaux présents dans la file d'attente
    pub queue_length: usize,
    /// Valeur courante d'update_id
    pub update_id: u32,
    /// Dernière modification (RFC3339)
    pub last_change: Option<String>,
    /// Nombre total d'entrées en historique (persisté)
    pub history_entries: usize,
    /// Limite configurée pour l'historique
    pub history_max_tracks: usize,
    /// Le canal est-il activé dans la configuration ?
    pub configured: bool,
    /// Identifiant de collection pour le cache
    pub cache_collection_id: String,
    /// Nombre total de pistes connues du cache
    pub cache_total_tracks: usize,
    /// Nombre de pistes déjà en cache
    pub cache_cached_tracks: usize,
}

/// Entrée détaillée de la file d'attente
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChannelPlaylistEntry {
    /// Position dans la file
    pub index: usize,
    /// ID unique de la piste
    pub track_id: String,
    /// ID du canal
    pub channel_id: u8,
    /// Titre du morceau
    pub title: String,
    /// Artiste
    pub artist: String,
    /// Album
    pub album: Option<String>,
    /// URL de couverture (si disponible)
    pub cover_url: Option<String>,
    /// Durée du morceau en ms
    pub duration_ms: u64,
    /// Offset dans le block (ms)
    pub elapsed_ms: u64,
    /// Horodatage prévu/démarré (RFC3339)
    pub started_at: String,
    /// Nombre de clients restants à servir
    pub pending_clients: usize,
    /// Note éventuelle (0-10)
    pub rating: Option<f32>,
    /// Année éventuelle
    pub year: Option<u32>,
    /// Statut de cache
    pub cache_status: CacheStatusInfo,
}

impl ChannelPlaylistEntry {
    fn from_entry(entry: &Arc<PlaylistEntry>, index: usize, cache_status: CacheStatusInfo) -> Self {
        let song = entry.song.as_ref();
        Self {
            index,
            track_id: entry.track_id.clone(),
            channel_id: entry.channel_id,
            title: song.title.clone(),
            artist: song.artist.clone(),
            album: song.album.clone(),
            cover_url: song.cover.clone(),
            duration_ms: entry.duration_ms,
            elapsed_ms: song.elapsed,
            started_at: entry.started_at.to_rfc3339(),
            pending_clients: entry.pending_clients(),
            rating: song.rating,
            year: song.year,
            cache_status,
        }
    }
}

/// Réponse pour la file d'attente d'un canal
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChannelPlaylistResponse {
    /// ID du canal
    pub channel_id: u8,
    /// Slug du canal
    pub slug: String,
    /// Update ID du playlist
    pub update_id: u32,
    /// Taille totale de la file au moment de la capture
    pub queue_length: usize,
    /// Entrées retournées
    pub items: Vec<ChannelPlaylistEntry>,
}

/// Entrée d'historique d'écoute
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChannelHistoryEntry {
    /// ID unique de la piste
    pub track_id: String,
    /// ID du canal
    pub channel_id: u8,
    /// Titre
    pub title: String,
    /// Artiste
    pub artist: String,
    /// Album
    pub album: Option<String>,
    /// URL de couverture
    pub cover_url: Option<String>,
    /// Début de lecture (RFC3339)
    pub started_at: String,
    /// Durée en ms
    pub duration_ms: u64,
}

/// Réponse pour l'historique d'un canal
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChannelHistoryResponse {
    /// ID du canal
    pub channel_id: u8,
    /// Slug du canal
    pub slug: String,
    /// Nombre total d'entrées disponibles
    pub total_available: usize,
    /// Nombre d'entrées retournées dans cette réponse
    pub returned: usize,
    /// Entrées
    pub entries: Vec<ChannelHistoryEntry>,
}

/// GET /channels/{channel_id}/status - Statut détaillé d'un canal
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/status",
    params(
        ("channel_id" = u8, Path, description = "Channel ID (0-3)")
    ),
    responses(
        (status = 200, description = "Statut du canal", body = ChannelStatusResponse),
        (status = 400, description = "Canal invalide"),
        (status = 503, description = "Canal indisponible"),
        (status = 500, description = "Erreur interne lors de la récupération du statut")
    ),
    tag = "Radio Paradise"
)]
async fn get_channel_status(
    State(state): State<RadioParadiseState>,
    Path(channel_id): Path<u8>,
) -> Result<Json<ChannelStatusResponse>, StatusCode> {
    let channel = state.channel_for_id(channel_id)?;
    let descriptor = channel.descriptor();

    let playlist = channel.playlist();
    let queue_length = playlist.active_len().await;
    let update_id = playlist.update_id();
    let last_change = playlist
        .last_change()
        .await
        .map(|ts| DateTime::<Utc>::from(ts).to_rfc3339());

    let history_len = channel.history_backend().len().await.map_err(|e| {
        error!(
            channel = descriptor.slug,
            "Failed to retrieve history size: {e:?}"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let cache_stats = channel.cache_manager().statistics().await;
    let config = channel.config().clone();
    let configured = config.channels.iter().any(|slug| slug == descriptor.slug);

    let status = ChannelStatusResponse {
        channel_id,
        slug: descriptor.slug.to_string(),
        name: descriptor.display_name.to_string(),
        description: descriptor.description.to_string(),
        active_clients: channel.active_client_count(),
        queue_length,
        update_id,
        last_change,
        history_entries: history_len,
        history_max_tracks: config.history.max_tracks,
        configured,
        cache_collection_id: cache_stats.collection_id,
        cache_total_tracks: cache_stats.total_tracks,
        cache_cached_tracks: cache_stats.cached_tracks,
    };

    Ok(Json(status))
}

/// GET /channels/{channel_id}/playlist - File d'attente du canal
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/playlist",
    params(
        ("channel_id" = u8, Path, description = "Channel ID (0-3)"),
        ListLimitQuery
    ),
    responses(
        (status = 200, description = "File d'attente courante", body = ChannelPlaylistResponse),
        (status = 400, description = "Canal invalide"),
        (status = 503, description = "Canal indisponible"),
        (status = 500, description = "Erreur lors de la récupération de la file d'attente")
    ),
    tag = "Radio Paradise"
)]
async fn get_channel_playlist(
    State(state): State<RadioParadiseState>,
    Path(channel_id): Path<u8>,
    Query(query): Query<ListLimitQuery>,
) -> Result<Json<ChannelPlaylistResponse>, StatusCode> {
    let channel = state.channel_for_id(channel_id)?;
    let descriptor = channel.descriptor();
    let playlist = channel.playlist();
    let snapshot = playlist.active_snapshot().await;
    let total_len = snapshot.len();
    let limit = query.limit.filter(|limit| *limit > 0).unwrap_or(total_len);

    let cache_manager = channel.cache_manager();
    let mut items = Vec::new();

    for (index, entry) in snapshot.into_iter().enumerate().take(limit) {
        let cache_status = match cache_manager.get_cache_status(&entry.track_id).await {
            Ok(status) => status,
            Err(err) => CacheStatus::Failed {
                error: err.to_string(),
            },
        };

        items.push(ChannelPlaylistEntry::from_entry(
            &entry,
            index,
            CacheStatusInfo::from(cache_status),
        ));
    }

    let response = ChannelPlaylistResponse {
        channel_id,
        slug: descriptor.slug.to_string(),
        update_id: playlist.update_id(),
        queue_length: total_len,
        items,
    };

    Ok(Json(response))
}

/// GET /channels/{channel_id}/history - Historique récent du canal
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/history",
    params(
        ("channel_id" = u8, Path, description = "Channel ID (0-3)"),
        ListLimitQuery
    ),
    responses(
        (status = 200, description = "Historique récent", body = ChannelHistoryResponse),
        (status = 400, description = "Canal invalide"),
        (status = 503, description = "Canal indisponible"),
        (status = 500, description = "Erreur lors de la récupération de l'historique")
    ),
    tag = "Radio Paradise"
)]
async fn get_channel_history(
    State(state): State<RadioParadiseState>,
    Path(channel_id): Path<u8>,
    Query(query): Query<ListLimitQuery>,
) -> Result<Json<ChannelHistoryResponse>, StatusCode> {
    let channel = state.channel_for_id(channel_id)?;
    let descriptor = channel.descriptor();
    let backend = channel.history_backend().clone();
    let limit = query.limit.unwrap_or(50);

    let entries_raw = backend.recent(limit).await.map_err(|e| {
        error!(
            channel = descriptor.slug,
            "Failed to retrieve channel history: {e:?}"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let total_available = backend.len().await.map_err(|e| {
        error!(
            channel = descriptor.slug,
            "Failed to count channel history entries: {e:?}"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let entries: Vec<ChannelHistoryEntry> = entries_raw
        .into_iter()
        .map(|entry| ChannelHistoryEntry {
            track_id: entry.track_id,
            channel_id: entry.channel_id,
            title: entry.song.title,
            artist: entry.song.artist,
            album: entry.song.album,
            cover_url: entry.song.cover_url,
            started_at: entry.started_at.to_rfc3339(),
            duration_ms: entry.duration_ms,
        })
        .collect();

    let response = ChannelHistoryResponse {
        channel_id,
        slug: descriptor.slug.to_string(),
        total_available,
        returned: entries.len(),
        entries,
    };

    Ok(Json(response))
}

/// GET /channels/{channel_id}/stream/{connection_id} - Stream audio pour une connexion spécifique
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/stream/{connection_id}",
    params(
        ("channel_id" = u8, Path, description = "Channel ID (0-3)"),
        ("connection_id" = i32, Path, description = "Connection ID fourni par le media server")
    ),
    responses(
        (status = 200, description = "Flux audio FLAC (gapless)", content_type = "audio/flac"),
        (status = 400, description = "Canal invalide"),
        (status = 503, description = "Canal indisponible")
    ),
    tag = "Radio Paradise"
)]
async fn stream_channel_by_connection(
    State(state): State<RadioParadiseState>,
    Path((channel_id, connection_id)): Path<(u8, i32)>,
) -> Result<impl IntoResponse, StatusCode> {
    let channel = state.channel_for_id(channel_id)?;

    // Convertir connection_id en String pour l'utiliser comme client_id
    let client_id = connection_id.to_string();

    let client_stream = channel.connect_client(client_id).await.map_err(|e| {
        error!("Failed to create streaming client: {e:?}");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let stream = client_stream
        .into_byte_stream()
        .map(|chunk| chunk.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));

    let body = Body::from_stream(stream);

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("audio/flac"),
    );
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    );
    headers.insert(
        HeaderName::from_static("icy-name"),
        HeaderValue::from_static("Radio Paradise"),
    );
    headers.insert(
        HeaderName::from_static("icy-genre"),
        HeaderValue::from_static("Eclectic"),
    );
    headers.insert(
        HeaderName::from_static("icy-description"),
        HeaderValue::from_static("PMO Radio Paradise relay"),
    );
    headers.insert(
        HeaderName::from_static("icy-metaint"),
        HeaderValue::from_static("0"),
    );

    Ok((headers, body))
}

/// Documentation OpenAPI pour l'API Radio Paradise
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Radio Paradise API",
        version = "1.0.0",
        description = "API REST pour accéder aux métadonnées et streams de Radio Paradise"
    ),
    paths(
        get_now_playing,
        get_current_block,
        get_block_by_id,
        get_channels,
        get_channel_status,
        get_channel_playlist,
        get_channel_history,
        stream_channel_by_connection
    ),
    components(schemas(
        NowPlayingResponse,
        BlockResponse,
        SongInfo,
        ChannelInfo,
        ChannelStatusResponse,
        ChannelPlaylistEntry,
        ChannelPlaylistResponse,
        ChannelHistoryEntry,
        ChannelHistoryResponse,
        CacheStatusInfo
    )),
    tags(
        (name = "Radio Paradise", description = "Endpoints pour Radio Paradise streaming")
    )
)]
pub struct RadioParadiseApiDoc;

/// Crée le router pour l'API Radio Paradise
pub fn create_api_router(state: RadioParadiseState) -> Router {
    Router::new()
        .route("/now-playing", get(get_now_playing))
        .route("/block/current", get(get_current_block))
        .route("/block/{event_id}", get(get_block_by_id))
        .route("/channels", get(get_channels))
        .route("/channels/{channel_id}/status", get(get_channel_status))
        .route("/channels/{channel_id}/playlist", get(get_channel_playlist))
        .route("/channels/{channel_id}/history", get(get_channel_history))
        .route(
            "/channels/{channel_id}/stream/{connection_id}",
            get(stream_channel_by_connection),
        )
        .with_state(state)
}

/// Trait d'extension pour pmoserver::Server
///
/// Permet d'initialiser Radio Paradise avec routes HTTP complètes
#[cfg(feature = "pmoserver")]
pub trait RadioParadiseExt {
    /// Initialise l'API Radio Paradise
    ///
    /// # Routes créées
    ///
    /// - API: `/api/radioparadise/*`
    ///   - `/now-playing`
    ///   - `/block/*`
    ///   - `/channels/{channel_id}/stream/{connection_id}`
    /// - Swagger: `/swagger-ui/radioparadise`
    async fn init_radioparadise(&mut self) -> anyhow::Result<RadioParadiseState>;
}

#[cfg(feature = "pmoserver")]
impl RadioParadiseExt for pmoserver::Server {
    async fn init_radioparadise(&mut self) -> anyhow::Result<RadioParadiseState> {
        let state = RadioParadiseState::new().await?;

        // Créer le router API
        let api_router = create_api_router(state.clone());

        // L'enregistrer avec OpenAPI
        self.add_openapi(api_router, RadioParadiseApiDoc::openapi(), "radioparadise")
            .await;

        Ok(state)
    }
}
