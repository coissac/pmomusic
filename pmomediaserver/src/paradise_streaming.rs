//! Extension pour l'initialisation des canaux de streaming Radio Paradise
//!
//! Ce module fournit un trait d'extension pour démarrer les pipelines de streaming
//! Radio Paradise avec caching audio/covers et historique.

use anyhow::{Context, Result};
use async_trait::async_trait;
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use pmoaudiocache::{AudioCacheExt, Cache as AudioCache, get_audio_cache, register_audio_cache};
use pmocovers::{Cache as CoverCache, CoverCacheExt, get_cover_cache, register_cover_cache};
use pmoparadise::{ParadiseChannelManager, ParadiseHistoryBuilder, channels::ALL_CHANNELS};
use pmoplaylist::register_audio_cache as register_playlist_audio_cache;
use std::sync::Arc;
use tokio_util::io::ReaderStream;
use tracing::{error, info};

/// État partagé pour les routes de streaming Paradise
#[derive(Clone)]
pub struct ParadiseStreamingState {
    pub manager: Arc<ParadiseChannelManager>,
}

/// Extension trait pour initialiser les canaux de streaming Radio Paradise
#[async_trait]
pub trait ParadiseStreamingExt {
    /// Initialise les canaux de streaming Radio Paradise avec caching
    ///
    /// Cette méthode :
    /// - Crée les caches audio et covers
    /// - Initialise le ParadiseChannelManager avec historique
    /// - Ajoute les routes de streaming HTTP (flac, ogg, history, metadata)
    ///
    /// # Routes créées
    ///
    /// Pour chaque canal (main, mellow, rock, eclectic) :
    /// - `/radioparadise/stream/{slug}/flac` - Stream FLAC live
    /// - `/radioparadise/stream/{slug}/ogg` - Stream OGG live
    /// - `/radioparadise/stream/{slug}/historic/{client_id}/flac` - Historique FLAC
    /// - `/radioparadise/stream/{slug}/historic/{client_id}/ogg` - Historique OGG
    /// - `/radioparadise/metadata/{slug}` - Métadonnées en temps réel
    ///
    /// # Exemples
    ///
    /// ```ignore
    /// use pmomediaserver::ParadiseStreamingExt;
    ///
    /// server.init_paradise_streaming().await?;
    /// ```
    async fn init_paradise_streaming(&mut self) -> Result<Arc<ParadiseChannelManager>>;
}

#[async_trait]
impl ParadiseStreamingExt for pmoserver::Server {
    async fn init_paradise_streaming(&mut self) -> Result<Arc<ParadiseChannelManager>> {
        info!("🎵 Initializing Radio Paradise streaming channels...");

        // Récupérer ou initialiser les caches singletons
        info!("📦 Getting cache singletons...");
        let cover_cache = match get_cover_cache() {
            Some(cache) => {
                info!("  ✅ Using existing cover cache singleton");
                cache
            }
            None => {
                info!("  📦 Initializing new cover cache singleton");
                let cache = self
                    .init_cover_cache_configured()
                    .await
                    .context("Failed to initialize cover cache")?;
                register_cover_cache(cache.clone());
                cache
            }
        };

        let audio_cache = match get_audio_cache() {
            Some(cache) => {
                info!("  ✅ Using existing audio cache singleton");
                // S'assurer qu'il est aussi enregistré dans le playlist manager
                register_playlist_audio_cache(cache.clone());
                cache
            }
            None => {
                info!("  📦 Initializing new audio cache singleton");
                let cache = self
                    .init_audio_cache_configured()
                    .await
                    .context("Failed to initialize audio cache")?;
                register_audio_cache(cache.clone());
                register_playlist_audio_cache(cache.clone());
                cache
            }
        };

        // Créer le builder d'historique
        let mut history_builder = ParadiseHistoryBuilder::default();
        history_builder.playlist_prefix = "radioparadise-history".into();
        history_builder.playlist_title_prefix = Some("Radio Paradise History".into());
        history_builder.max_history_tracks = Some(500);
        history_builder.collection_prefix = Some("radioparadise".into());
        history_builder.replay_max_lead_seconds = 1.0;

        // Créer le manager de canaux
        info!("📡 Creating ParadiseChannelManager...");
        let manager = Arc::new(
            ParadiseChannelManager::with_defaults_with_cover_cache(
                Some(cover_cache.clone()),
                Some(history_builder),
            )
            .await
            .context("Failed to create ParadiseChannelManager")?,
        );

        let state = Arc::new(ParadiseStreamingState {
            manager: manager.clone(),
        });

        // Ajouter les routes pour chaque canal
        info!("🌐 Registering streaming routes...");
        for descriptor in ALL_CHANNELS.iter() {
            let slug = descriptor.slug;
            let channel_id = descriptor.id;

            // Route FLAC live
            let flac_path = format!("/radioparadise/stream/{}/flac", slug);
            self.add_handler_with_state(
                &flac_path,
                move |State(state): State<Arc<ParadiseStreamingState>>| {
                    let manager = state.manager.clone();
                    async move { stream_flac(manager, channel_id).await }
                },
                state.clone(),
            )
            .await;

            // Route OGG live
            let ogg_path = format!("/radioparadise/stream/{}/ogg", slug);
            self.add_handler_with_state(
                &ogg_path,
                move |State(state): State<Arc<ParadiseStreamingState>>| {
                    let manager = state.manager.clone();
                    async move { stream_ogg(manager, channel_id).await }
                },
                state.clone(),
            )
            .await;

            // Routes historique
            let history_path = format!("/radioparadise/stream/{}/historic", slug);
            let history_router = Router::new()
                .route(
                    "/{client_id}/flac",
                    get({
                        let manager = manager.clone();
                        move |Path(client_id): Path<String>| {
                            let manager = manager.clone();
                            async move { stream_history_flac(manager, channel_id, client_id).await }
                        }
                    }),
                )
                .route(
                    "/{client_id}/ogg",
                    get({
                        let manager = manager.clone();
                        move |Path(client_id): Path<String>| {
                            let manager = manager.clone();
                            async move { stream_history_ogg(manager, channel_id, client_id).await }
                        }
                    }),
                );

            self.add_router(&history_path, history_router).await;

            // Route métadonnées
            let meta_path = format!("/radioparadise/metadata/{}", slug);
            self.add_handler_with_state(
                &meta_path,
                move |State(state): State<Arc<ParadiseStreamingState>>| {
                    let manager = state.manager.clone();
                    async move { get_metadata(manager, channel_id).await }
                },
                state.clone(),
            )
            .await;

            info!(
                "  ✅ {} - /radioparadise/stream/{}/{{flac,ogg}}",
                descriptor.display_name, slug
            );
        }

        info!("✅ Radio Paradise streaming channels initialized");

        Ok(manager)
    }
}

// ============================================================================
// Handlers de streaming
// ============================================================================

async fn stream_flac(
    manager: Arc<ParadiseChannelManager>,
    channel_id: u8,
) -> Result<Response, StatusCode> {
    let channel = manager.get(channel_id).ok_or(StatusCode::NOT_FOUND)?;
    let stream = channel.subscribe_flac();
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "audio/flac")
        .body(Body::from_stream(ReaderStream::new(stream)))
        .unwrap())
}

async fn stream_ogg(
    manager: Arc<ParadiseChannelManager>,
    channel_id: u8,
) -> Result<Response, StatusCode> {
    let channel = manager.get(channel_id).ok_or(StatusCode::NOT_FOUND)?;
    let stream = channel.subscribe_ogg();
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "audio/ogg")
        .body(Body::from_stream(ReaderStream::new(stream)))
        .unwrap())
}

async fn get_metadata(
    manager: Arc<ParadiseChannelManager>,
    channel_id: u8,
) -> Result<impl IntoResponse, StatusCode> {
    let channel = manager.get(channel_id).ok_or(StatusCode::NOT_FOUND)?;
    let metadata = channel.metadata().await;
    Ok(Json(metadata))
}

async fn stream_history_flac(
    manager: Arc<ParadiseChannelManager>,
    channel_id: u8,
    client_id: String,
) -> Result<Response, StatusCode> {
    let channel = manager.get(channel_id).ok_or(StatusCode::NOT_FOUND)?;
    let stream = channel.stream_history_flac(&client_id).await.map_err(|e| {
        error!(
            "Failed to start historical FLAC stream for channel {} (client_id={}): {}",
            channel_id, client_id, e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "audio/flac")
        .body(Body::from_stream(ReaderStream::new(stream)))
        .unwrap())
}

async fn stream_history_ogg(
    manager: Arc<ParadiseChannelManager>,
    channel_id: u8,
    client_id: String,
) -> Result<Response, StatusCode> {
    let channel = manager.get(channel_id).ok_or(StatusCode::NOT_FOUND)?;
    let stream = channel.stream_history_ogg(&client_id).await.map_err(|e| {
        error!(
            "Failed to start historical OGG stream for channel {} (client_id={}): {}",
            channel_id, client_id, e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "audio/ogg")
        .body(Body::from_stream(ReaderStream::new(stream)))
        .unwrap())
}
