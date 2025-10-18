//! Extension pmoserver pour Radio Paradise
//!
//! Ce module fournit un trait d'extension pour ajouter facilement l'API Radio Paradise
//! à un serveur pmoserver.

use crate::{RadioParadiseClient, Block, NowPlaying};
use axum::{
    extract::{Path, State},
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

impl RadioParadiseState {
    pub async fn new() -> anyhow::Result<Self> {
        let client = RadioParadiseClient::new()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create RadioParadise client: {}", e))?;
        Ok(Self {
            client: Arc::new(RwLock::new(client)),
        })
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
                album: song.album.clone(),
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
                album: song.album.clone(),
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
                album: song.album.clone(),
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
    responses(
        (status = 200, description = "Morceau en cours", body = NowPlayingResponse),
        (status = 500, description = "Erreur serveur")
    ),
    tag = "Radio Paradise"
)]
async fn get_now_playing(
    State(state): State<RadioParadiseState>,
) -> Result<Json<NowPlayingResponse>, StatusCode> {
    let client = state.client.read().await;
    let now_playing = client
        .now_playing()
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch now playing from Radio Paradise: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(now_playing.into()))
}

/// GET /block/current - Récupère le block actuel
#[utoipa::path(
    get,
    path = "/block/current",
    responses(
        (status = 200, description = "Block actuel", body = BlockResponse),
        (status = 500, description = "Erreur serveur")
    ),
    tag = "Radio Paradise"
)]
async fn get_current_block(
    State(state): State<RadioParadiseState>,
) -> Result<Json<BlockResponse>, StatusCode> {
    let client = state.client.read().await;
    let block = client
        .get_block(None)
        .await
        .map_err(|e| {
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
        ("event_id" = u64, Path, description = "Event ID du block")
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
) -> Result<Json<BlockResponse>, StatusCode> {
    let client = state.client.read().await;
    let block = client
        .get_block(Some(event_id))
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch block {} from Radio Paradise: {}", event_id, e);
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
    let channels = vec![
        ChannelInfo {
            id: 0,
            name: "Main Mix".to_string(),
            description: "Eclectic mix of rock, world, electronica, and more".to_string(),
        },
        ChannelInfo {
            id: 1,
            name: "Mellow Mix".to_string(),
            description: "Mellower, less aggressive music".to_string(),
        },
        ChannelInfo {
            id: 2,
            name: "Rock Mix".to_string(),
            description: "Heavier, more guitar-driven music".to_string(),
        },
        ChannelInfo {
            id: 3,
            name: "World/Etc Mix".to_string(),
            description: "Global beats and world music".to_string(),
        },
    ];

    Json(channels)
}

/// Information sur un bitrate
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BitrateInfo {
    /// ID du bitrate (0-4)
    pub id: u8,
    /// Nom/description
    pub name: String,
}

/// GET /bitrates - Liste les bitrates disponibles
#[utoipa::path(
    get,
    path = "/bitrates",
    responses(
        (status = 200, description = "Liste des bitrates disponibles", body = Vec<BitrateInfo>)
    ),
    tag = "Radio Paradise"
)]
async fn get_bitrates() -> Json<Vec<BitrateInfo>> {
    let bitrates = vec![
        BitrateInfo {
            id: 0,
            name: "MP3 128 kbps".to_string(),
        },
        BitrateInfo {
            id: 1,
            name: "AAC 64 kbps".to_string(),
        },
        BitrateInfo {
            id: 2,
            name: "AAC 128 kbps".to_string(),
        },
        BitrateInfo {
            id: 3,
            name: "AAC 320 kbps".to_string(),
        },
        BitrateInfo {
            id: 4,
            name: "FLAC Lossless".to_string(),
        },
    ];

    Json(bitrates)
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
        get_bitrates
    ),
    components(schemas(
        NowPlayingResponse,
        BlockResponse,
        SongInfo,
        ChannelInfo,
        BitrateInfo
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
        .route("/bitrates", get(get_bitrates))
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
        self.add_openapi(
            api_router,
            RadioParadiseApiDoc::openapi(),
            "radioparadise"
        ).await;

        Ok(state)
    }
}
