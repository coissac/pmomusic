//! Simple web server that exposes one Radio Paradise channel over HTTP.
//!
//! Usage:
//! ```bash
//! cargo run --example single_channel_server --features full -- main
//! ```
//! Valid arguments are either the slug (`main`, `mellow`, `rock`, `eclectic`) or
//! the numeric channel id (`0`..`3`). When no argument is provided, the example
//! defaults to the “main” mix.

use axum::{
    body::Body, extract::State, http::StatusCode, response::Response, routing::get, Router,
};
use pmoaudiocache::{
    new_cache_with_consolidation as new_audio_cache,
    register_audio_cache as register_global_audio_cache,
};
use pmocovers::{new_cache_with_consolidation as new_cover_cache, register_cover_cache};
use pmoparadise::{
    channels::{ChannelDescriptor, ALL_CHANNELS},
    ParadiseHistoryBuilder, ParadiseStreamChannel, ParadiseStreamChannelConfig,
};
use pmoplaylist::register_audio_cache as register_playlist_audio_cache;
use std::{fs, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;
use tracing::info;

#[derive(Clone)]
struct AppState {
    channel: Arc<ParadiseStreamChannel>,
    descriptor: ChannelDescriptor,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let descriptor = pick_descriptor(std::env::args().nth(1))?;
    info!(
        "Selected Radio Paradise channel: {} ({})",
        descriptor.display_name, descriptor.slug
    );

    // Prepare caches under ./cache/single-channel
    let cache_root = "./cache/single-channel";
    let audio_cache_dir = format!("{}/audio", cache_root);
    let cover_cache_dir = format!("{}/covers", cache_root);
    fs::create_dir_all(&audio_cache_dir)?;
    fs::create_dir_all(&cover_cache_dir)?;

    let audio_cache = new_audio_cache(&audio_cache_dir, 1000).await?;
    let cover_cache = new_cover_cache(&cover_cache_dir, 200).await?;
    register_global_audio_cache(audio_cache.clone());
    register_playlist_audio_cache(audio_cache.clone());
    register_cover_cache(cover_cache.clone());

    let mut history_builder = ParadiseHistoryBuilder::new(audio_cache.clone(), cover_cache.clone());
    history_builder.playlist_prefix = format!("single-channel-history-{}", descriptor.slug);
    history_builder.collection_prefix = Some(format!("single-channel-{}", descriptor.slug));
    let history_opts = history_builder.build_for_channel(&descriptor).await?;

    let channel = Arc::new(
        ParadiseStreamChannel::new(
            descriptor,
            ParadiseStreamChannelConfig::default(),
            Some(cover_cache),
            Some(history_opts),
        )
        .await?,
    );

    let state = AppState {
        channel,
        descriptor,
    };

    let app = Router::new()
        .route("/stream/flac", get(stream_flac))
        .route("/stream/ogg", get(stream_ogg))
        .with_state(state);

    let addr: SocketAddr = ([0, 0, 0, 0], 8080).into();
    info!("HTTP server listening on http://{addr}/stream/flac and /stream/ogg");
    info!("Connect with a FLAC player (e.g. ffplay http://localhost:8080/stream/flac)");
    info!("Connect with an OGG/OGG-FLAC player (e.g. ffplay http://localhost:8080/stream/ogg)");

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}

async fn stream_flac(State(state): State<AppState>) -> Result<Response, StatusCode> {
    let stream = state.channel.subscribe_flac();
    let body = Body::from_stream(ReaderStream::new(stream));
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "audio/flac")
        .header(
            "X-PMO-Channel",
            format!(
                "{} ({})",
                state.descriptor.display_name, state.descriptor.slug
            ),
        )
        .body(body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn stream_ogg(State(state): State<AppState>) -> Result<Response, StatusCode> {
    let stream = state.channel.subscribe_ogg();
    let body = Body::from_stream(ReaderStream::new(stream));
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "audio/ogg")
        .header(
            "X-PMO-Channel",
            format!(
                "{} ({})",
                state.descriptor.display_name, state.descriptor.slug
            ),
        )
        .body(body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn pick_descriptor(arg: Option<String>) -> anyhow::Result<ChannelDescriptor> {
    if let Some(token) = arg {
        if let Some(desc) = ALL_CHANNELS.iter().find(|c| c.slug == token) {
            return Ok(*desc);
        }
        if let Ok(id) = token.parse::<u8>() {
            if let Some(desc) = ALL_CHANNELS.iter().find(|c| c.id == id) {
                return Ok(*desc);
            }
        }
        anyhow::bail!("Unknown channel identifier: {token}");
    }
    Ok(ALL_CHANNELS[0])
}
