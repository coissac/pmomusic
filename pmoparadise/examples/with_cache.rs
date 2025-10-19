//! Example demonstrating Radio Paradise with cache support
//!
//! This example shows how to use the RadioParadiseSource with pmocovers
//! and pmoaudiocache to cache both cover images and audio tracks.
//!
//! Run with:
//! ```bash
//! cargo run --example with_cache --features cache
//! ```

use pmoaudiocache::AudioCache;
use pmocovers::Cache as CoverCache;
use pmoparadise::{RadioParadiseClient, RadioParadiseSource};
use pmosource::MusicSource;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("🎵 Radio Paradise with Cache Support");
    println!("=====================================\n");

    // Create the Radio Paradise client
    println!("📡 Connecting to Radio Paradise...");
    let client = RadioParadiseClient::new().await?;
    println!("✅ Connected!\n");

    // Initialize caches
    println!("💾 Initializing caches...");
    let cover_cache = Arc::new(CoverCache::new("./cache/covers", 500)?);
    let audio_cache = Arc::new(AudioCache::new("./cache/audio", 100)?);
    println!("✅ Caches initialized!\n");

    // Create the source with caching enabled
    let source = RadioParadiseSource::new_with_cache(
        client.clone(),
        "http://localhost:8080",
        50,
        Some(cover_cache.clone()),
        Some(audio_cache.clone()),
    );

    println!("📻 Source: {}", source.name());
    println!("🆔 ID: {}", source.id());
    println!("📝 Supports FIFO: {}\n", source.supports_fifo());

    // Fetch current playing information
    println!("🎧 Fetching current track information...");
    let now_playing = client.now_playing().await?;
    let block = Arc::new(now_playing.block.clone());

    println!("\n🎵 Now Playing:");
    println!("   Event: {}", block.event);
    if let Some(song) = &now_playing.current_song {
        println!("   Title: {}", song.title);
        println!("   Artist: {}", song.artist);
        println!("   Album: {}", song.album);
    }
    println!();

    // Add current song to the source
    println!("➕ Adding current track to FIFO with caching...");
    if let Some(song) = &now_playing.current_song {
        source
            .add_song(
                block.clone(),
                song,
                now_playing.current_song_index.unwrap_or(0),
            )
            .await?;
        println!("✅ Track added and caching started!");
        println!("   - Cover image will be cached to: ./cache/covers/");
        println!("   - Audio will be cached to: ./cache/audio/\n");
    }

    // Wait a bit for caching to start
    println!("⏳ Waiting for cache operations to complete...");
    sleep(Duration::from_secs(5)).await;

    // Get items from FIFO
    println!("\n📋 Items in FIFO:");
    let items = source.get_items(0, 10).await?;
    for (i, item) in items.iter().enumerate() {
        println!(
            "   {}. {} - {}",
            i + 1,
            item.artist.as_deref().unwrap_or("Unknown"),
            item.title
        );

        // Show resolved URI (will use cached version if available)
        if let Ok(uri) = source.resolve_uri(&item.id).await {
            println!("      URI: {}", uri);
        }
    }

    println!("\n✨ Example complete!");
    println!("\n💡 Tips:");
    println!("   - Run the example again to see faster loading from cache");
    println!("   - Check ./cache/covers/ for cached cover images");
    println!("   - Check ./cache/audio/ for cached FLAC files");

    Ok(())
}
