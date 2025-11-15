//! Minimal HTTP server exposing all four Radio Paradise channels.
//!
//! Routes:
//! - `/radioparadise/stream/<slug>/flac`
//! - `/radioparadise/stream/<slug>/ogg`
//! - `/radioparadise/stream/<slug>/icy`
//! - `/radioparadise/metadata/<slug>`

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use pmoparadise::{channels::ALL_CHANNELS, stream_channel::ParadiseChannelManager};
use pmoserver::{init_logging, ServerBuilder};
use tokio_util::io::ReaderStream;
use tracing::info;

#[derive(Clone)]
struct AppState {
    manager: Arc<ParadiseChannelManager>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = init_logging();

    info!("Initializing Radio Paradise channels...");
    let manager = Arc::new(ParadiseChannelManager::with_defaults().await?);
    let app_state = Arc::new(AppState {
        manager: manager.clone(),
    });

    let mut server = ServerBuilder::new("RadioParadiseChannels", "http://localhost", 8080).build();

    for descriptor in ALL_CHANNELS.iter() {
        let slug = descriptor.slug;
        let flac_path = format!("/radioparadise/stream/{}/flac", slug);
        let ogg_path = format!("/radioparadise/stream/{}/ogg", slug);
        let icy_path = format!("/radioparadise/stream/{}/icy", slug);
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
            "  {}: /radioparadise/stream/{}/flac (also /ogg, /icy, metadata)",
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
