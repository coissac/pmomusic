/// Test example to debug playlist attachment issues
///
/// This example tests each step of attaching a playlist to an OpenHome renderer:
/// 1. Browse the MediaServer for playlist items
/// 2. Insert each item into the OpenHome renderer's playlist
/// 3. Verify the operation succeeded
///
/// Usage:
///   cargo run --example test_attach_playlist

use anyhow::{Context, Result};
use pmocontrol::{
    media_server::{MediaBrowser, MediaEntry, MediaServer, ServerId},
    openhome_client::OhPlaylistClient,
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,pmocontrol=debug")),
        )
        .init();

    info!("🧪 Starting playlist attachment test");

    // Configuration - adjust these for your setup
    let media_server_url = "http://192.168.0.138:8080";
    let media_server_id = "uuid:17fe2ea6-8908-4e30-bc52-b28ea4cab3e4";
    let playlist_container_id = "radio-paradise:channel:mellow:liveplaylist";

    let renderer_playlist_url = "http://192.168.0.200:49152/Playlist";
    let renderer_service_type = "urn:av-openhome-org:service:Playlist:1";

    info!("Configuration:");
    info!("  MediaServer: {}", media_server_url);
    info!("  Playlist: {}", playlist_container_id);
    info!("  Renderer: {}", renderer_playlist_url);
    info!("");

    // Step 1: Create MediaServer client
    info!("📡 Step 1: Connecting to MediaServer");
    let content_directory_url = format!(
        "{}/device/{}/service/ContentDirectory/control",
        media_server_url, media_server_id.replace("uuid:", "")
    );

    let server = MediaServer::new(
        ServerId(media_server_id.to_string()),
        "PMOMusic Test".to_string(),
        content_directory_url,
    );

    debug!("MediaServer client created");

    // Step 2: Browse the playlist container
    info!("📂 Step 2: Browsing playlist container");
    info!("  Container ID: {}", playlist_container_id);

    let entries = match server.browse_children(playlist_container_id, 0, 10) {
        Ok(entries) => {
            info!("✅ Browse succeeded: {} items found", entries.len());
            entries
        }
        Err(e) => {
            error!("❌ Browse failed: {}", e);
            error!("   This is where the error occurs!");
            return Err(e);
        }
    };

    if entries.is_empty() {
        warn!("⚠️  Playlist is empty, nothing to insert");
        return Ok(());
    }

    // Display the first few items
    info!("📋 First items in playlist:");
    for (idx, entry) in entries.iter().take(3).enumerate() {
        info!("  [{}] {} - {}",
            idx,
            entry.title,
            entry.artist.as_deref().unwrap_or("Unknown")
        );
        if let Some(res) = entry.resources.first() {
            debug!("      URI: {}", res.uri);
            debug!("      protocolInfo: {}", res.protocol_info);
        }
    }
    info!("");

    // Step 3: Connect to OpenHome renderer
    info!("🎵 Step 3: Connecting to OpenHome renderer");
    let oh_client = OhPlaylistClient::new(
        renderer_playlist_url.to_string(),
        renderer_service_type.to_string(),
    );
    debug!("OpenHome client created");

    // Step 4: Clear existing playlist
    info!("🗑️  Step 4: Clearing existing playlist");
    match oh_client.delete_all() {
        Ok(_) => info!("✅ Playlist cleared"),
        Err(e) => {
            warn!("⚠️  Could not clear playlist: {}", e);
        }
    }
    info!("");

    // Step 5: Insert items one by one
    info!("➕ Step 5: Inserting items into renderer");
    let mut after_id = 0u32;
    let mut inserted_count = 0;

    for (idx, entry) in entries.iter().enumerate() {
        if entry.is_container {
            debug!("Skipping container: {}", entry.title);
            continue;
        }

        let resource = match entry.resources.iter().find(|r| r.is_audio()) {
            Some(r) => r,
            None => {
                warn!("No audio resource found for: {}", entry.title);
                continue;
            }
        };

        info!("  [{}] Inserting: {}", idx, entry.title);
        debug!("      URI: {}", resource.uri);
        debug!("      protocolInfo: {}", resource.protocol_info);

        // Build simple DIDL-Lite metadata
        let didl_metadata = format!(
            r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/"><item id="{}" parentID="-1" restricted="1"><dc:title>{}</dc:title><upnp:class>object.item.audioItem.musicTrack</upnp:class><res protocolInfo="{}">{}</res></item></DIDL-Lite>"#,
            entry.id,
            xmlescape(&entry.title),
            resource.protocol_info,
            xmlescape(&resource.uri)
        );

        trace!("DIDL metadata: {}", didl_metadata);

        match oh_client.insert(after_id, &resource.uri, &didl_metadata) {
            Ok(new_id) => {
                debug!("      ✅ Inserted with ID: {}", new_id);
                after_id = new_id;
                inserted_count += 1;
            }
            Err(e) => {
                error!("      ❌ Insert failed: {}", e);
                error!("         This is the UPnP 501 error location!");

                // Continue with next item instead of failing
                warn!("      Continuing with next item...");
            }
        }
    }

    info!("");
    info!("✅ Test completed: {}/{} items inserted successfully",
        inserted_count, entries.len());

    Ok(())
}

/// Simple XML escaping
fn xmlescape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
