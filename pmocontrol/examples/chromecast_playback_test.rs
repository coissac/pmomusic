//! Simple Chromecast playback test
//!
//! This example tests basic Chromecast functionality by:
//! 1. Connecting to a Chromecast device
//! 2. Launching the DefaultMediaReceiver app
//! 3. Loading and playing a test media URL
//! 4. Maintaining the heartbeat loop
//!
//! Usage:
//!   cargo run --example chromecast_playback_test -- <chromecast_ip> [media_url]
//!
//! Example:
//!   cargo run --example chromecast_playback_test -- 192.168.1.100

use std::env;
use std::sync::Once;

use rust_cast::{
    CastDevice, ChannelMessage,
    channels::{
        heartbeat::HeartbeatResponse,
        media::{Media, StreamType},
        receiver::CastDeviceApp,
    },
};

const DEFAULT_DESTINATION_ID: &str = "receiver-0";
const DEFAULT_PORT: u16 = 8009;

// Test media URLs (public domain audio files)
const TEST_MEDIA_URL: &str = "https://www.soundhelix.com/examples/mp3/SoundHelix-Song-1.mp3";

/// Ensures the Rustls CryptoProvider is initialized exactly once.
fn ensure_crypto_provider_initialized() {
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        let _ = rustls::crypto::CryptoProvider::install_default(
            rustls::crypto::aws_lc_rs::default_provider(),
        );
        println!("✓ Rustls CryptoProvider initialized");
    });
}

/// Detects content type from URL path
fn detect_content_type(url: &str) -> String {
    // Detect from URL path - check if path contains /flac/, /mp3/, etc.
    if url.contains("/flac/") || url.contains(".flac") {
        println!("  ✓ Detected FLAC from URL path");
        return "audio/flac".to_string();
    }
    if url.contains("/mp3/") || url.contains(".mp3") {
        println!("  ✓ Detected MP3 from URL path");
        return "audio/mpeg".to_string();
    }
    if url.contains(".m4a") || url.contains(".mp4") || url.contains(".aac") {
        println!("  ✓ Detected AAC/M4A from URL path");
        return "audio/mp4".to_string();
    }
    if url.contains("/ogg/") || url.contains(".ogg") {
        println!("  ✓ Detected OGG from URL path");
        return "audio/ogg".to_string();
    }
    if url.contains(".opus") {
        println!("  ✓ Detected Opus from URL path");
        return "audio/opus".to_string();
    }
    if url.contains(".wav") {
        println!("  ✓ Detected WAV from URL path");
        return "audio/wav".to_string();
    }

    // Fallback
    println!("  ⚠ Could not detect type, using audio/mpeg as fallback");
    "audio/mpeg".to_string()
}

fn main() {
    // Setup logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <chromecast_ip> [media_url]", args[0]);
        eprintln!("\nExample:");
        eprintln!("  {} 192.168.1.100", args[0]);
        eprintln!(
            "\nIf no media URL is provided, will use: {}",
            TEST_MEDIA_URL
        );
        std::process::exit(1);
    }

    let chromecast_ip = &args[1];
    let media_url = if args.len() > 2 {
        &args[2]
    } else {
        TEST_MEDIA_URL
    };

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║      Chromecast Playback Test (rust_cast)                 ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();
    println!("Target:      {}:{}", chromecast_ip, DEFAULT_PORT);
    println!("Media URL:   {}", media_url);
    println!();
    println!("Detecting media content type...");
    let media_type = detect_content_type(media_url);
    println!();

    // Initialize crypto provider
    ensure_crypto_provider_initialized();

    // Step 1: Connect to the device
    println!("──────────────────────────────────────────────────────────");
    println!("STEP 1: Connecting to Chromecast...");
    let cast_device =
        match CastDevice::connect_without_host_verification(chromecast_ip, DEFAULT_PORT) {
            Ok(device) => {
                println!("✓ Connected to Chromecast");
                device
            }
            Err(e) => {
                eprintln!("✗ Failed to connect: {}", e);
                std::process::exit(1);
            }
        };

    // Step 2: Connect to the default receiver channel
    println!();
    println!("STEP 2: Connecting to receiver channel...");
    if let Err(e) = cast_device
        .connection
        .connect(DEFAULT_DESTINATION_ID.to_string())
    {
        eprintln!("✗ Failed to connect channel: {}", e);
        std::process::exit(1);
    }
    println!("✓ Channel connected");

    // Step 3: Send initial ping (CRITICAL per rust_caster.rs)
    println!();
    println!("STEP 3: Sending initial heartbeat ping...");
    if let Err(e) = cast_device.heartbeat.ping() {
        eprintln!("✗ Failed to send initial ping: {}", e);
        std::process::exit(1);
    }
    println!("✓ Initial ping sent");

    // Step 4: Get receiver status
    println!();
    println!("STEP 4: Getting receiver status...");
    let status = match cast_device.receiver.get_status() {
        Ok(status) => {
            println!("✓ Receiver status obtained");
            println!(
                "  - Volume: {:.0}%",
                status.volume.level.unwrap_or(0.5) * 100.0
            );
            println!("  - Muted: {}", status.volume.muted.unwrap_or(false));
            println!("  - Running apps: {}", status.applications.len());
            status
        }
        Err(e) => {
            eprintln!("✗ Failed to get status: {}", e);
            std::process::exit(1);
        }
    };

    // Step 5: Launch DefaultMediaReceiver
    println!();
    println!("STEP 5: Launching DefaultMediaReceiver app...");
    let app = match cast_device
        .receiver
        .launch_app(&CastDeviceApp::DefaultMediaReceiver)
    {
        Ok(app) => {
            println!("✓ App launched successfully");
            println!("  - App ID: {}", app.app_id);
            println!("  - Display Name: {}", app.display_name);
            println!("  - Session ID: {}", app.session_id);
            println!("  - Transport ID: {}", app.transport_id);
            app
        }
        Err(e) => {
            eprintln!("✗ Failed to launch app: {}", e);
            std::process::exit(1);
        }
    };

    // Step 6: Connect to the app's transport
    println!();
    println!("STEP 6: Connecting to app transport...");
    if let Err(e) = cast_device.connection.connect(app.transport_id.as_str()) {
        eprintln!("✗ Failed to connect to app transport: {}", e);
        std::process::exit(1);
    }
    println!("✓ Connected to app transport: {}", app.transport_id);

    // Step 7: Load the media
    println!();
    println!("STEP 7: Loading media...");
    println!("  Content-Type: {}", media_type);
    let media = Media {
        content_id: media_url.to_string(),
        content_type: media_type,
        stream_type: StreamType::Buffered,
        duration: None,
        metadata: None,
    };

    match cast_device
        .media
        .load(app.transport_id.as_str(), app.session_id.as_str(), &media)
    {
        Ok(status) => {
            println!("✓ Media loaded successfully!");
            println!("  - Media status entries: {}", status.entries.len());

            if let Some(entry) = status.entries.first() {
                println!("  - Player state: {:?}", entry.player_state);
                println!("  - Media session ID: {}", entry.media_session_id);

                if let Some(ref media) = entry.media {
                    println!("  - Content ID: {}", media.content_id);
                    println!("  - Stream type: {:?}", media.stream_type);
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to load media: {}", e);
            std::process::exit(1);
        }
    }

    // Step 8: Enter heartbeat loop
    println!();
    println!("──────────────────────────────────────────────────────────");
    println!("STEP 8: Entering heartbeat loop (Ctrl+C to exit)...");
    println!("──────────────────────────────────────────────────────────");
    println!();

    let mut heartbeat_count = 0;
    let mut media_message_count = 0;

    loop {
        match cast_device.receive() {
            Ok(ChannelMessage::Heartbeat(response)) => {
                if let HeartbeatResponse::Ping = response {
                    heartbeat_count += 1;
                    println!(
                        "[Heartbeat #{:3}] Received Ping, sending Pong...",
                        heartbeat_count
                    );

                    if let Err(e) = cast_device.heartbeat.pong() {
                        eprintln!("✗ Failed to send pong: {}", e);
                        break;
                    }
                } else {
                    println!("[Heartbeat] {:?}", response);
                }
            }

            Ok(ChannelMessage::Media(response)) => {
                media_message_count += 1;
                println!("[Media #{:3}] {:?}", media_message_count, response);
            }

            Ok(ChannelMessage::Receiver(response)) => {
                println!("[Receiver] {:?}", response);
            }

            Ok(ChannelMessage::Connection(response)) => {
                println!("[Connection] {:?}", response);
            }

            Ok(ChannelMessage::Raw(response)) => {
                println!("[Raw] Unsupported message type: {:?}", response);
            }

            Err(e) => {
                eprintln!("✗ Error receiving message: {}", e);
                break;
            }
        }
    }

    println!();
    println!("──────────────────────────────────────────────────────────");
    println!("Test completed.");
    println!("Total heartbeats: {}", heartbeat_count);
    println!("Total media messages: {}", media_message_count);
}
