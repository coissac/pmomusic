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

/// Documentation OpenAPI pour l'API Radio Paradise
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Radio Paradise API",
        version = "1.0.0",
        description = "API REST pour accéder aux métadonnées de Radio Paradise"
    ),
    paths(
        get_now_playing,
        get_current_block,
        get_block_by_id,
        get_channels
    ),
    components(schemas(
        NowPlayingResponse,
        BlockResponse,
        SongInfo,
        ChannelInfo
    )),
    tags(
        (name = "Radio Paradise", description = "Endpoints pour Radio Paradise")
    )
)]
pub struct RadioParadiseApiDoc;

/// Crée le router pour l'API Radio Paradise
pub fn create_api_router(state: RadioParadiseState) -> Router {
    let api = Router::new()
        .route("/now-playing", get(get_now_playing))
        .route("/block/current", get(get_current_block))
        .route("/block/{event_id}", get(get_block_by_id))
        .route("/channels", get(get_channels))
        .with_state(state);

    Router::new().nest("/radioparadise", api)
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
