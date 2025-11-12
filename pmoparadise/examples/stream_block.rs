//! Streams a Radio Paradise block via HTTP using pmoserver
//!
//! This example demonstrates streaming a single Radio Paradise block
//! using the StreamingFlacSink over HTTP via pmoserver. Perfect for
//! testing with VLC or other media players that support HTTP streaming.
//!
//! Architecture:
//! ```text
//! RadioParadiseStreamSource → TimerNode → StreamingFlacSink
//!                                             ↓
//!                                       StreamHandle
//!                                             ↓
//!                                     pmoserver (Axum)
//!                                             ↓
//!                                 VLC / Media Player Client
//! ```
//!
//! Usage:
//!   cargo run --example stream_block --features full -- <channel_id>
//!
//! Example:
//!   cargo run --example stream_block --features full -- 0    # Main Mix
//!
//! Then open in VLC:
//!   vlc http://localhost:8080/test/stream           (pure FLAC)
//!   vlc http://localhost:8080/test/stream-ogg       (OGG-FLAC streaming container)
//!   vlc http://localhost:8080/test/stream-icy       (FLAC + ICY metadata)
//!
//! To check current metadata:
//!   curl http://localhost:8080/test/metadata

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use pmoaudio::{AudioPipelineNode, TimerNode};
use pmoaudio_ext::{StreamingFlacSink, StreamingOggFlacSink};
use pmoflac::EncoderOptions;
use pmoparadise::{RadioParadiseClient, RadioParadiseStreamSource};
use pmoserver::{ServerBuilder, init_logging};
use std::env;
use std::sync::Arc;
use tokio_util::io::ReaderStream;
use tokio_util::sync::CancellationToken;

/// Shared application state
struct AppState {
    stream_handle: pmoaudio_ext::StreamHandle,
    ogg_handle: pmoaudio_ext::OggFlacStreamHandle,
}

/// Main HTTP handler for streaming (pure FLAC, no ICY metadata)
async fn stream_handler(
    State(state): State<Arc<AppState>>,
    _headers: HeaderMap,
) -> Result<Response, StatusCode> {
    tracing::info!("New client connected (pure FLAC mode)");

    // Pure FLAC stream without ICY metadata
    let flac_stream = state.stream_handle.subscribe_flac();

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "audio/flac")
        .header("Cache-Control", "no-cache, no-store")
        .body(Body::from_stream(ReaderStream::new(flac_stream)))
        .unwrap())
}

/// ICY streaming handler (FLAC with embedded metadata)
async fn stream_icy_handler(
    State(state): State<Arc<AppState>>,
    _headers: HeaderMap,
) -> Result<Response, StatusCode> {
    tracing::info!("New client connected (ICY mode)");

    // FLAC stream with ICY metadata
    let icy_stream = state.stream_handle.subscribe_icy();

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "audio/flac")
        .header("icy-metaint", "16000")
        .header("icy-name", "Radio Paradise Stream Test")
        .header("icy-genre", "Eclectic")
        .header("icy-pub", "1")
        .header("Cache-Control", "no-cache, no-store")
        .body(Body::from_stream(ReaderStream::new(icy_stream)))
        .unwrap())
}

/// OGG-FLAC streaming handler
async fn stream_ogg_handler(
    State(state): State<Arc<AppState>>,
    _headers: HeaderMap,
) -> Result<Response, StatusCode> {
    tracing::info!("New client connected (OGG-FLAC mode)");

    // OGG-FLAC stream
    let ogg_stream = state.ogg_handle.subscribe();

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "audio/ogg")
        .header("Cache-Control", "no-cache, no-store")
        .body(Body::from_stream(ReaderStream::new(ogg_stream)))
        .unwrap())
}

/// Metadata endpoint (JSON)
async fn metadata_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let metadata = state.stream_handle.get_metadata().await;
    axum::Json(metadata)
}

/// Health check endpoint
async fn health_handler() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging via pmoserver
    let _log_state = init_logging();

    tracing::info!("=== Radio Paradise HTTP Streaming Test ===");

    // Parse arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <channel_id>", args[0]);
        eprintln!();
        eprintln!("Streams a Radio Paradise block via HTTP for testing.");
        eprintln!();
        eprintln!("Channel IDs:");
        eprintln!("  0 - Main Mix (eclectic, diverse mix)");
        eprintln!("  1 - Mellow Mix (smooth, chilled music)");
        eprintln!("  2 - Rock Mix (classic & modern rock)");
        eprintln!("  3 - World/Etc Mix (global sounds)");
        eprintln!();
        eprintln!("After starting, open in VLC:");
        eprintln!("  vlc http://localhost:8080/test/stream           (pure FLAC)");
        eprintln!("  vlc http://localhost:8080/test/stream-ogg       (OGG-FLAC container)");
        eprintln!("  vlc http://localhost:8080/test/stream-icy       (FLAC + ICY metadata)");
        std::process::exit(1);
    }

    let channel_id: u8 = match args[1].parse() {
        Ok(id) if id <= 3 => id,
        _ => {
            eprintln!("Error: channel_id must be a number between 0 and 3");
            std::process::exit(1);
        }
    };

    tracing::info!("Channel ID: {}", channel_id);

    // ═══════════════════════════════════════════════════════════════════════════
    // Fetch block metadata
    // ═══════════════════════════════════════════════════════════════════════════

    tracing::info!("Fetching current block metadata...");
    let client = RadioParadiseClient::builder()
        .channel(channel_id)
        .build()
        .await?;

    let block = client.get_block(None).await?;

    tracing::info!("Block Information:");
    tracing::info!("  Event ID: {}", block.event);
    tracing::info!("  Songs: {}", block.song_count());
    tracing::info!("  Duration: {:.1} minutes", block.length as f64 / 60000.0);
    tracing::info!("");

    tracing::info!("Tracklist:");
    for (index, song) in block.songs_ordered() {
        tracing::info!(
            "  {:2}. {} - {} ({})",
            index + 1,
            song.artist,
            song.title,
            song.album.as_deref().unwrap_or("Unknown Album")
        );
    }
    tracing::info!("");

    // ═══════════════════════════════════════════════════════════════════════════
    // Create streaming pipelines (FLAC and OGG-FLAC)
    // ═══════════════════════════════════════════════════════════════════════════

    tracing::info!("Creating streaming pipelines...");

    // Encoder options (shared)
    let encoder_options = EncoderOptions {
        compression_level: 5,
        verify: false,
        ..Default::default()
    };

    // ─────────────────────────────────────────────────────────────────────────
    // Pipeline 1: FLAC streaming
    // ─────────────────────────────────────────────────────────────────────────

    let mut source_flac = RadioParadiseStreamSource::new(client.clone());
    source_flac.push_block_id(block.event);
    tracing::debug!("RadioParadiseStreamSource (FLAC) created with block {}", block.event);

    let mut timer_flac = TimerNode::new(3.0);
    tracing::debug!("TimerNode (FLAC) created with 3.0s max lead time");

    let (streaming_sink, stream_handle) = StreamingFlacSink::new(encoder_options.clone(), 16);
    tracing::debug!("StreamingFlacSink created");

    timer_flac.register(Box::new(streaming_sink));
    source_flac.register(Box::new(timer_flac));
    tracing::info!("Pipeline 1 connected: RadioParadiseStreamSource → TimerNode → StreamingFlacSink");

    // ─────────────────────────────────────────────────────────────────────────
    // Pipeline 2: OGG-FLAC streaming
    // ─────────────────────────────────────────────────────────────────────────

    let mut source_ogg = RadioParadiseStreamSource::new(client);
    source_ogg.push_block_id(block.event);
    tracing::debug!("RadioParadiseStreamSource (OGG) created with block {}", block.event);

    let mut timer_ogg = TimerNode::new(3.0);
    tracing::debug!("TimerNode (OGG) created with 3.0s max lead time");

    let (ogg_sink, ogg_handle) = StreamingOggFlacSink::new(encoder_options, 16);
    tracing::debug!("StreamingOggFlacSink created");

    timer_ogg.register(Box::new(ogg_sink));
    source_ogg.register(Box::new(timer_ogg));
    tracing::info!("Pipeline 2 connected: RadioParadiseStreamSource → TimerNode → StreamingOggFlacSink");

    // ═══════════════════════════════════════════════════════════════════════════
    // Setup pmoserver with streaming routes
    // ═══════════════════════════════════════════════════════════════════════════

    tracing::info!("Setting up pmoserver...");

    let mut server = ServerBuilder::new("RadioParadiseStreamTest", "http://localhost", 8080)
        .build();

    let app_state = Arc::new(AppState {
        stream_handle,
        ogg_handle,
    });

    // Add streaming routes
    server.add_handler_with_state("/test/stream", stream_handler, app_state.clone()).await;
    server.add_handler_with_state("/test/stream-icy", stream_icy_handler, app_state.clone()).await;
    server.add_handler_with_state("/test/stream-ogg", stream_ogg_handler, app_state.clone()).await;

    // Add metadata route
    server.add_handler_with_state("/test/metadata", metadata_handler, app_state.clone()).await;

    // Add health check
    server.add_handler("/test/health", health_handler).await;

    tracing::info!("");
    tracing::info!("========================================");
    tracing::info!("Ready to stream!");
    tracing::info!("");
    tracing::info!("Pure FLAC stream (for VLC, standard players):");
    tracing::info!("  vlc http://localhost:8080/test/stream");
    tracing::info!("");
    tracing::info!("OGG-FLAC stream (streaming container with metadata support):");
    tracing::info!("  vlc http://localhost:8080/test/stream-ogg");
    tracing::info!("");
    tracing::info!("FLAC + ICY metadata stream (for ICY-aware clients):");
    tracing::info!("  http://localhost:8080/test/stream-icy");
    tracing::info!("");
    tracing::info!("Metadata endpoint (JSON):");
    tracing::info!("  curl http://localhost:8080/test/metadata");
    tracing::info!("========================================");
    tracing::info!("");

    // ═══════════════════════════════════════════════════════════════════════════
    // Start pipelines and server
    // ═══════════════════════════════════════════════════════════════════════════

    let stop_token = CancellationToken::new();
    let stop_token_flac = stop_token.clone();
    let stop_token_ogg = stop_token.clone();

    // Start FLAC pipeline in background
    let pipeline_flac_handle = tokio::spawn(async move {
        tracing::info!("[PIPELINE-FLAC] Starting...");
        let result = Box::new(source_flac).run(stop_token_flac).await;
        match &result {
            Ok(()) => tracing::info!("[PIPELINE-FLAC] Completed successfully"),
            Err(e) => tracing::error!("[PIPELINE-FLAC] Error: {}", e),
        }
        result
    });

    // Start OGG-FLAC pipeline in background
    let pipeline_ogg_handle = tokio::spawn(async move {
        tracing::info!("[PIPELINE-OGG] Starting...");
        let result = Box::new(source_ogg).run(stop_token_ogg).await;
        match &result {
            Ok(()) => tracing::info!("[PIPELINE-OGG] Completed successfully"),
            Err(e) => tracing::error!("[PIPELINE-OGG] Error: {}", e),
        }
        result
    });

    // Start pmoserver (blocks until Ctrl+C)
    tracing::info!("[SERVER] Starting pmoserver...");
    server.start().await;
    server.wait().await;

    // Server stopped, cancel pipelines
    tracing::info!("Server stopped, canceling pipelines...");
    stop_token.cancel();

    // Wait for both pipelines to finish
    match pipeline_flac_handle.await {
        Ok(Ok(())) => tracing::info!("FLAC pipeline completed successfully"),
        Ok(Err(e)) => tracing::error!("FLAC pipeline error: {}", e),
        Err(e) => tracing::error!("FLAC pipeline task error: {}", e),
    }

    match pipeline_ogg_handle.await {
        Ok(Ok(())) => tracing::info!("OGG-FLAC pipeline completed successfully"),
        Ok(Err(e)) => tracing::error!("OGG-FLAC pipeline error: {}", e),
        Err(e) => tracing::error!("OGG-FLAC pipeline task error: {}", e),
    }

    tracing::info!("Shutdown complete");
    Ok(())
}
