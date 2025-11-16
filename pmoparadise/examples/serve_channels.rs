//! Minimal HTTP server exposing all four Radio Paradise channels.
//!
//! Routes:
//! - `/radioparadise/stream/<slug>/flac`
//! - `/radioparadise/stream/<slug>/ogg`
//! - `/radioparadise/stream/<slug>/icy`
//! - `/radioparadise/stream/<slug>/historic/<client_id>/flac`
//! - `/radioparadise/stream/<slug>/historic/<client_id>/ogg`
//! - `/radioparadise/metadata/<slug>`

use std::{fs, sync::Arc};

use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use pmoaudiocache::{
    new_cache_with_consolidation as new_audio_cache,
    register_audio_cache as register_global_audio_cache,
};
use pmocovers::{new_cache_with_consolidation as new_cover_cache, register_cover_cache};
use pmoparadise::{channels::ALL_CHANNELS, ParadiseChannelManager, ParadiseHistoryBuilder};
use pmoplaylist::register_audio_cache as register_playlist_audio_cache;
use pmoserver::{init_logging, ServerBuilder};
use tokio_util::io::ReaderStream;
use tracing::{error, info};

#[derive(Clone)]
struct AppState {
    manager: Arc<ParadiseChannelManager>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = init_logging();

    // Préparer les caches partagés
    let cover_cache_dir = "./cache/rp_covers";
    let audio_cache_dir = "./cache/rp_audio";
    fs::create_dir_all(cover_cache_dir)?;
    fs::create_dir_all(audio_cache_dir)?;

    let cover_cache = new_cover_cache(cover_cache_dir, 500).await?;
    let audio_cache = new_audio_cache(audio_cache_dir, 1000).await?;
    register_global_audio_cache(audio_cache.clone());
    register_playlist_audio_cache(audio_cache.clone());
    register_cover_cache(cover_cache.clone());
    let _playlist_manager = pmoplaylist::PlaylistManager();

    let history_builder = ParadiseHistoryBuilder {
        audio_cache: audio_cache.clone(),
        cover_cache: cover_cache.clone(),
        playlist_prefix: "radioparadise-history".into(),
        playlist_title_prefix: Some("Radio Paradise History".into()),
        max_history_tracks: Some(500),
        collection_prefix: Some("radioparadise".into()),
        replay_max_lead_seconds: 1.0,
    };

    info!("Initializing Radio Paradise channels...");
    let manager = Arc::new(
        ParadiseChannelManager::with_defaults_with_cover_cache(
            Some(cover_cache),
            Some(history_builder),
        )
        .await?,
    );
    let app_state = Arc::new(AppState {
        manager: manager.clone(),
    });

    let mut server = ServerBuilder::new("RadioParadiseChannels", "http://localhost", 8080).build();

    for descriptor in ALL_CHANNELS.iter() {
        let slug = descriptor.slug;
        let flac_path = format!("/radioparadise/stream/{}/flac", slug);
        let ogg_path = format!("/radioparadise/stream/{}/ogg", slug);
        let icy_path = format!("/radioparadise/stream/{}/icy", slug);
        let history_path = format!("/radioparadise/stream/{}/historic", slug);
        let meta_path = format!("/radioparadise/metadata/{}", slug);
        let channel_id = descriptor.id;

        server
            .add_handler_with_state(
                &flac_path,
                move |State(state): State<Arc<AppState>>| {
                    let manager = state.manager.clone();
                    async move { stream_flac(manager, channel_id).await }
                },
                app_state.clone(),
            )
            .await;

        server
            .add_handler_with_state(
                &ogg_path,
                move |State(state): State<Arc<AppState>>| {
                    let manager = state.manager.clone();
                    async move { stream_ogg(manager, channel_id).await }
                },
                app_state.clone(),
            )
            .await;

        server
            .add_handler_with_state(
                &icy_path,
                move |State(state): State<Arc<AppState>>| {
                    let manager = state.manager.clone();
                    async move { stream_icy(manager, channel_id).await }
                },
                app_state.clone(),
            )
            .await;

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

        server.add_router(&history_path, history_router).await;

        server
            .add_handler_with_state(
                &meta_path,
                move |State(state): State<Arc<AppState>>| {
                    let manager = state.manager.clone();
                    async move { get_metadata(manager, channel_id).await }
                },
                app_state.clone(),
            )
            .await;
    }

    info!("========================================");
    info!("Radio Paradise streaming server running on http://localhost:8080");
    info!("Available channels:");
    for descriptor in ALL_CHANNELS.iter() {
        info!(
            "  {}: /radioparadise/stream/{}/flac (also /ogg, /icy, metadata, /historic/<client_id>/(flac|ogg))",
            descriptor.display_name, descriptor.slug
        );
    }
    info!("Press Ctrl+C to stop.");
    info!("========================================");

    server.start().await;
    server.wait().await;
    Ok(())
}

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

async fn stream_icy(
    manager: Arc<ParadiseChannelManager>,
    channel_id: u8,
) -> Result<Response, StatusCode> {
    let channel = manager.get(channel_id).ok_or(StatusCode::NOT_FOUND)?;
    let stream = channel.subscribe_icy();
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "audio/flac")
        .header("icy-metaint", "16000")
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
