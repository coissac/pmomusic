//! Example: Run a UPnP/DLNA Media Server for Radio Paradise
//!
//! This example demonstrates:
//! - Creating a UPnP Media Server
//! - Exposing Radio Paradise blocks and songs
//! - SSDP discovery and announcements
//! - ContentDirectory and ConnectionManager services
//!
//! Run with: cargo run --example upnp_mediaserver --features mediaserver
//!
//! The server will be discoverable by DLNA/UPnP clients on your network.

#[cfg(feature = "mediaserver")]
use pmoparadise::mediaserver::RadioParadiseMediaServer;
#[cfg(feature = "mediaserver")]
use pmoparadise::Bitrate;

#[cfg(feature = "mediaserver")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    #[cfg(feature = "logging")]
    tracing_subscriber::fmt::init();

    println!("Radio Paradise UPnP Media Server");
    println!("=================================\n");

    // Create the media server
    println!("Creating media server...");
    let server = RadioParadiseMediaServer::builder()
        .with_friendly_name("Radio Paradise FLAC")
        .with_manufacturer("PMOMusic")
        .with_model_name("Radio Paradise Adapter v0.1")
        .with_bitrate(Bitrate::Flac)
        .with_channel(0) // Main mix
        .with_port(8080)
        .build()
        .await?;

    println!("Media Server created!");
    println!("  UDN: {}", server.udn());
    println!("  Port: 8080");
    println!("  Quality: FLAC Lossless");
    println!("  Channel: Main Mix (0)");
    println!();

    println!("Server is now discoverable on your network.");
    println!("Look for 'Radio Paradise FLAC' in your DLNA/UPnP clients.");
    println!();
    println!("ContentDirectory service available at:");
    println!(
        "  http://localhost:8080/upnp/device/{}/service/ContentDirectory",
        server.udn()
    );
    println!();
    println!("Press Ctrl+C to stop the server.");
    println!();

    // Run the server
    server.run().await?;

    Ok(())
}

#[cfg(not(feature = "mediaserver"))]
fn main() {
    eprintln!("ERROR: This example requires the 'mediaserver' feature.");
    eprintln!("Run with: cargo run --example upnp_mediaserver --features mediaserver");
    std::process::exit(1);
}
