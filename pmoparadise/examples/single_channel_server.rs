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
    body::Body,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use pmoaudio_ext::StreamingSinkOptions;
use pmoaudiocache::{
    new_cache_with_consolidation as new_audio_cache,
    register_audio_cache as register_global_audio_cache,
};
use pmocovers::{
    new_cache_with_consolidation as new_cover_cache, register_cover_cache, Cache as CoverCache,
};
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
    cover_cache: Arc<CoverCache>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt().with_env_filter(env_filter).init();

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

    let mut channel_config = ParadiseStreamChannelConfig::default();
    // Base URL for cover images in stream metadata
    let server_base_url = "http://localhost:8080".to_string();

    // Configuration commune pour FLAC et OGG
    let common_options = StreamingSinkOptions::flac_defaults()
        .with_default_artist(Some("Radio Paradise".to_string()))
        .with_default_title(descriptor.display_name.to_string())
        .with_server_base_url(Some(server_base_url.clone()));

    channel_config.flac_options = common_options.clone();
    channel_config.ogg_options = StreamingSinkOptions::ogg_defaults()
        .with_default_artist(Some("Radio Paradise".to_string()))
        .with_default_title(descriptor.display_name.to_string())
        .with_server_base_url(Some(server_base_url));

    let channel = Arc::new(
        ParadiseStreamChannel::new(
            descriptor,
            channel_config,
            Some(cover_cache.clone()),
            Some(history_opts),
        )
        .await?,
    );

    let state = AppState {
        channel,
        descriptor,
        cover_cache,
    };

    let app = Router::new()
        .route("/stream/flac", get(stream_flac))
        .route("/stream/ogg", get(stream_ogg))
        .route("/metadata", get(get_metadata))
        .route("/covers/image/{pk}", get(get_cover))
        .with_state(state);

    let addr: SocketAddr = ([0, 0, 0, 0], 8080).into();
    info!("========================================");
    info!("HTTP server listening on http://{addr}");
    info!("Available endpoints:");
    info!("  - /stream/flac          : FLAC audio stream");
    info!("  - /stream/ogg           : OGG-FLAC audio stream");
    info!("  - /metadata             : Current track metadata (JSON)");
    info!("  - /covers/image/{{pk}}   : Album cover images (WebP)");
    info!("========================================");
    info!("Connect with a FLAC player: ffplay http://localhost:8080/stream/flac");
    info!("Connect with an OGG-FLAC player: ffplay http://localhost:8080/stream/ogg");

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

async fn get_metadata(
    State(state): State<AppState>,
    request: Request,
) -> Result<impl IntoResponse, StatusCode> {
    let mut metadata = state.channel.metadata().await;

    // Si cover_pk est disponible, construire l'URL complète depuis les headers
    // Format: /covers/image/{pk} (correspond à la structure du cache pmocovers)
    if let Some(ref pk) = metadata.cover_pk {
        let base_url = extract_base_url(&request);
        metadata.cover_url = Some(format!("{}/covers/image/{}", base_url, pk));
    }

    Ok(Json(metadata))
}

/// Extrait l'URL de base depuis les headers HTTP de la requête
/// Supporte les proxies avec X-Forwarded-Host et X-Forwarded-Proto
fn extract_base_url(request: &Request) -> String {
    let headers = request.headers();

    // Déterminer le schéma (http ou https)
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("http");

    // Déterminer le host
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");

    format!("{}://{}", scheme, host)
}

async fn get_cover(
    State(state): State<AppState>,
    Path(pk): Path<String>,
) -> Result<Response, StatusCode> {
    // Récupérer le chemin de la cover depuis le cache
    // Le cache retourne un PathBuf pointant vers le fichier .webp
    let cover_path = state.cover_cache.get(&pk).await.map_err(|e| {
        tracing::error!("Failed to get cover path for {}: {}", pk, e);
        StatusCode::NOT_FOUND
    })?;

    // Lire le fichier
    let cover_data = tokio::fs::read(&cover_path).await.map_err(|e| {
        tracing::error!("Failed to read cover file {:?}: {}", cover_path, e);
        StatusCode::NOT_FOUND
    })?;

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "image/webp")
        .header("Cache-Control", "public, max-age=86400")
        .body(Body::from(cover_data))
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
