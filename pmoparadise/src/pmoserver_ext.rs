//! Extension pmoserver pour Radio Paradise
//!
//! Ce module fournit un trait d'extension pour ajouter facilement l'API Radio Paradise
//! à un serveur pmoserver.

use crate::channels::{max_channel_id, ChannelDescriptor, ALL_CHANNELS};
use crate::{Block, NowPlaying, RadioParadiseClient};
use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use utoipa::{OpenApi, ToSchema};

/// État partagé pour l'API Radio Paradise
#[derive(Clone)]
pub struct RadioParadiseState {
    client: Arc<RwLock<RadioParadiseClient>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ParadiseQuery {
    channel: Option<u8>,
}

impl RadioParadiseState {
    pub async fn new() -> anyhow::Result<Self> {
        let client = RadioParadiseClient::new()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create RadioParadise client: {}", e))?;

        Ok(Self {
            client: Arc::new(RwLock::new(client)),
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

impl From<&ChannelDescriptor> for ChannelInfo {
    fn from(descriptor: &ChannelDescriptor) -> Self {
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

/// Réponse pour l'URL de streaming
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StreamUrlResponse {
    /// Event ID du block
    #[schema(example = 1234567)]
    pub event: u64,
    /// URL de streaming FLAC
    #[schema(example = "https://apps.radioparadise.com/blocks/chan/0/4/1234567-1234580.flac")]
    pub stream_url: String,
    /// Durée totale (ms)
    #[schema(example = 900000)]
    pub length_ms: u64,
}

/// Réponse pour l'URL de pochette
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CoverUrlResponse {
    /// Event ID du block
    #[schema(example = 1234567)]
    pub event: u64,
    /// Index du morceau
    #[schema(example = 0)]
    pub song_index: usize,
    /// URL de la pochette (résolution complète)
    #[schema(example = "https://img.radioparadise.com/covers/l/B00000I0JF.jpg")]
    pub cover_url: Option<String>,
    /// Type de pochette: "cover" (petite) ou "cover_large" (grande)
    #[schema(example = "cover_large")]
    pub cover_type: String,
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

/// GET /block/{event_id}/song/{index} - Récupère un morceau spécifique d'un block
#[utoipa::path(
    get,
    path = "/block/{event_id}/song/{index}",
    params(
        ("event_id" = u64, Path, description = "Event ID du block"),
        ("index" = usize, Path, description = "Index du morceau (0-based)"),
        ("channel" = Option<u8>, Query, description = "Channel ID (0-3)")
    ),
    responses(
        (status = 200, description = "Morceau demandé", body = SongInfo),
        (status = 404, description = "Morceau non trouvé"),
        (status = 500, description = "Erreur serveur")
    ),
    tag = "Radio Paradise"
)]
async fn get_song_by_index(
    State(state): State<RadioParadiseState>,
    Path((event_id, index)): Path<(u64, usize)>,
    Query(params): Query<ParadiseQuery>,
) -> Result<Json<SongInfo>, StatusCode> {
    let client = state.client_for_params(&params).await?;
    let block = client.get_block(Some(event_id)).await.map_err(|e| {
        tracing::error!(
            "Failed to fetch block {} from Radio Paradise: {}",
            event_id,
            e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let song = block.get_song(index).ok_or_else(|| {
        tracing::warn!("Song index {} not found in block {}", index, event_id);
        StatusCode::NOT_FOUND
    })?;

    let song_info = SongInfo {
        index,
        artist: song.artist.clone(),
        title: song.title.clone(),
        album: song.album.clone().unwrap_or_default(),
        year: song.year,
        elapsed_ms: song.elapsed,
        duration_ms: song.duration,
        cover_url: song.cover.as_ref().and_then(|c| block.cover_url(c)),
        rating: song.rating,
    };

    Ok(Json(song_info))
}

/// GET /cover-url/{event_id}/{song_index} - Récupère l'URL de la pochette d'un morceau
///
/// Utilise automatiquement cover_large si disponible, sinon cover en fallback
#[utoipa::path(
    get,
    path = "/cover-url/{event_id}/{song_index}",
    params(
        ("event_id" = u64, Path, description = "Event ID du block"),
        ("song_index" = usize, Path, description = "Index du morceau (0-based)"),
        ("channel" = Option<u8>, Query, description = "Channel ID (0-3)")
    ),
    responses(
        (status = 200, description = "URL de la pochette avec fallback automatique", body = CoverUrlResponse),
        (status = 404, description = "Morceau non trouvé"),
        (status = 500, description = "Erreur serveur")
    ),
    tag = "Radio Paradise"
)]
async fn get_cover_url(
    State(state): State<RadioParadiseState>,
    Path((event_id, song_index)): Path<(u64, usize)>,
    Query(params): Query<ParadiseQuery>,
) -> Result<Json<CoverUrlResponse>, StatusCode> {
    let client = state.client_for_params(&params).await?;
    let block = client.get_block(Some(event_id)).await.map_err(|e| {
        tracing::error!(
            "Failed to fetch block {} from Radio Paradise: {}",
            event_id,
            e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let song = block.get_song(song_index).ok_or_else(|| {
        tracing::warn!("Song index {} not found in block {}", song_index, event_id);
        StatusCode::NOT_FOUND
    })?;

    // Fallback: cover_large → cover → none
    let (cover_url, cover_type) = if let Some(ref cover_large) = song.cover_large {
        (block.cover_url(cover_large), "cover_large")
    } else if let Some(ref cover) = song.cover {
        (block.cover_url(cover), "cover")
    } else {
        (None, "none")
    };

    Ok(Json(CoverUrlResponse {
        event: event_id,
        song_index,
        cover_url,
        cover_type: cover_type.to_string(),
    }))
}

/// GET /stream-url/{event_id} - Récupère l'URL de streaming direct d'un block
#[utoipa::path(
    get,
    path = "/stream-url/{event_id}",
    params(
        ("event_id" = u64, Path, description = "Event ID du block (None pour le block actuel)"),
        ("channel" = Option<u8>, Query, description = "Channel ID (0-3)")
    ),
    responses(
        (status = 200, description = "URL de streaming", body = StreamUrlResponse),
        (status = 500, description = "Erreur serveur")
    ),
    tag = "Radio Paradise"
)]
async fn get_stream_url(
    State(state): State<RadioParadiseState>,
    Path(event_id): Path<u64>,
    Query(params): Query<ParadiseQuery>,
) -> Result<Json<StreamUrlResponse>, StatusCode> {
    let client = state.client_for_params(&params).await?;
    let block = client.get_block(Some(event_id)).await.map_err(|e| {
        tracing::error!(
            "Failed to fetch block {} from Radio Paradise: {}",
            event_id,
            e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(StreamUrlResponse {
        event: block.event,
        stream_url: block.url,
        length_ms: block.length,
    }))
}

/// Documentation OpenAPI pour l'API Radio Paradise
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Radio Paradise API",
        version = "1.0.0",
        description = r#"
# API REST pour Radio Paradise

Cette API permet d'accéder aux métadonnées et flux de Radio Paradise.

## Fonctionnalités

- **Métadonnées en temps réel** : Récupération du morceau en cours et des blocks
- **Multi-canaux** : Support des 4 canaux Radio Paradise (Main, Mellow, Rock, Eclectic)
- **Streaming FLAC** : Accès direct aux URLs de streaming haute qualité
- **Pochettes d'albums** : URLs complètes des couvertures (petite et grande taille)
- **Historique** : Accès aux blocks passés via event_id

## Canaux disponibles

- **0: Main Mix** - Eclectic mix of rock, world, electronica, and more
- **1: Mellow Mix** - Mellower, less aggressive music
- **2: Rock Mix** - Heavier, more guitar-driven music
- **3: Eclectic Mix** - Curated worldwide selection

## Format des données

### Blocks
Les blocks sont des fichiers FLAC continus contenant plusieurs morceaux.
Chaque block a un `event` (ID de début) et `end_event` (ID du prochain block).

### Timing
- Tous les temps sont en millisecondes (ms)
- `elapsed_ms` : temps écoulé depuis le début du block
- `duration_ms` : durée du morceau

## Exemples d'utilisation

### Récupérer le morceau en cours
```
GET /api/radioparadise/now-playing?channel=0
```

### Récupérer un block spécifique
```
GET /api/radioparadise/block/1234567?channel=0
```

### Récupérer la pochette d'un morceau (avec fallback automatique)
```
GET /api/radioparadise/cover-url/1234567/0?channel=0
```
        "#
    ),
    paths(
        get_now_playing,
        get_current_block,
        get_block_by_id,
        get_channels,
        get_song_by_index,
        get_cover_url,
        get_stream_url
    ),
    components(schemas(
        NowPlayingResponse,
        BlockResponse,
        SongInfo,
        ChannelInfo,
        StreamUrlResponse,
        CoverUrlResponse
    )),
    tags(
        (name = "Radio Paradise", description = "Endpoints pour Radio Paradise")
    )
)]
pub struct RadioParadiseApiDoc;

/// Crée le router pour l'API Radio Paradise
pub fn create_api_router(state: RadioParadiseState) -> Router {
    Router::new()
        .route("/now-playing", get(get_now_playing))
        .route("/block/current", get(get_current_block))
        .route("/block/{event_id}", get(get_block_by_id))
        .route("/block/{event_id}/song/{index}", get(get_song_by_index))
        .route("/cover-url/{event_id}/{song_index}", get(get_cover_url))
        .route("/stream-url/{event_id}", get(get_stream_url))
        .route("/channels", get(get_channels))
        .with_state(state)
}

/// Trait d'extension pour pmoserver::Server
///
/// Permet d'initialiser Radio Paradise avec routes HTTP complètes
#[cfg(feature = "pmoserver")]
#[async_trait]
pub trait RadioParadiseExt {
    /// Initialise l'API Radio Paradise
    ///
    /// # Routes créées
    ///
    /// - API: `/api/radioparadise/*`
    ///   - `/now-playing`
    ///   - `/block/*`
    ///   - `/channels`
    /// - Swagger: `/swagger-ui/radioparadise`
    async fn init_radioparadise(&mut self) -> anyhow::Result<RadioParadiseState>;
}

#[cfg(feature = "pmoserver")]
#[async_trait]
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
