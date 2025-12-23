//! Example demonstrating Qobuz with cache support
//!
//! This example shows how to use the QobuzSource with pmocovers
//! and pmoaudiocache to cache both cover images and audio tracks.
//!
//! Run with:
//! ```bash
//! cargo run --example with_cache --features cache
//! ```

use pmoaudiocache::Cache as AudioCache;
use pmocovers::Cache as CoverCache;
use pmoqobuz::{QobuzClient, QobuzSource};
use pmosource::MusicSource;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("🎵 Qobuz with Cache Support");
    println!("============================\n");

    // Create the Qobuz client using configuration
    println!("📡 Connecting to Qobuz...");
    let client = QobuzClient::from_config().await?;
    println!("✅ Connected!\n");

    // Initialize caches
    println!("💾 Initializing caches...");
    let cover_cache = Arc::new(CoverCache::new("./cache/qobuz-covers", 500)?);
    let audio_cache = Arc::new(AudioCache::new("./cache/qobuz-audio", 100)?);
    println!("✅ Caches initialized!\n");

    // Create the source with caching enabled
    let source = QobuzSource::new(client, cover_cache.clone(), audio_cache.clone());

    println!("📻 Source: {}", source.name());
    println!("🆔 ID: {}", source.id());
    println!("📝 Supports FIFO: {}\n", source.supports_fifo());

    // Get user's favorite tracks
    println!("🎧 Fetching your favorite tracks...");
    let favorite_tracks = source.client().get_favorite_tracks().await?;

    if favorite_tracks.is_empty() {
        println!("⚠️  No favorite tracks found. Add some favorites on Qobuz first!");
        println!("\n💡 Tip: You can also search for tracks:");

        // Example: Search for tracks
        println!("\n🔍 Searching for 'Miles Davis'...");
        let search_results = source.client().search("Miles Davis", None).await?;

        if !search_results.tracks.is_empty() {
            println!("\n📋 Found {} tracks:", search_results.tracks.len());
            for (i, track) in search_results.tracks.iter().enumerate().take(3) {
                println!(
                    "   {}. {} - {}",
                    i + 1,
                    track
                        .performer
                        .as_ref()
                        .map(|p| p.name.as_str())
                        .unwrap_or("Unknown"),
                    track.title
                );

                // Demonstrate adding a track with caching
                if i == 0 {
                    println!("\n➕ Adding first track to cache...");
                    let track_id = source.add_track(track).await?;
                    println!("✅ Track added with ID: {}", track_id);
                    println!("   - Cover image caching started");
                    println!("   - Audio caching started (high-quality FLAC)");

                    // Show resolved URI (will use cached version if available)
                    if let Ok(uri) = source.resolve_uri(&track_id).await {
                        println!("   - Stream URI: {}", uri);
                    }
                }
            }
        }
    } else {
        println!("✅ Found {} favorite tracks!\n", favorite_tracks.len());

        // Add first 3 favorite tracks with caching
        for (i, track) in favorite_tracks.iter().enumerate().take(3) {
            println!(
                "{}. {} - {}",
                i + 1,
                track
                    .performer
                    .as_ref()
                    .map(|p| p.name.as_str())
                    .unwrap_or("Unknown"),
                track.title
            );

            if let Some(album) = &track.album {
                println!("   Album: {}", album.title);
                if let Some(label) = &album.label {
                    println!("   Label: {}", label);
                }
                if let Some(sample_rate) = album.maximum_sampling_rate {
                    println!("   Max Sample Rate: {} kHz", sample_rate / 1000.0);
                }
                if let Some(bit_depth) = album.maximum_bit_depth {
                    println!("   Max Bit Depth: {} bit", bit_depth);
                }
            }

            println!("\n   ➕ Adding to cache...");
            match source.add_track(track).await {
                Ok(track_id) => {
                    println!("   ✅ Track cached successfully!");

                    // Show resolved URI
                    if let Ok(uri) = source.resolve_uri(&track_id).await {
                        println!("   📍 Stream URI: {}", uri);
                    }
                }
                Err(e) => {
                    println!("   ⚠️  Failed to cache track: {}", e);
                }
            }
            println!();
        }
    }

    // Browse favorite albums
    println!("\n📚 Browsing your favorite albums...");
    let favorite_albums = source.client().get_favorite_albums().await?;

    if !favorite_albums.is_empty() {
        println!("✅ Found {} favorite albums!\n", favorite_albums.len());

        for (i, album) in favorite_albums.iter().enumerate().take(3) {
            println!("{}. {} - {}", i + 1, album.artist.name, album.title);
            if let Some(release_date) = &album.release_date {
                println!("   Released: {}", release_date);
            }
            if let Some(tracks_count) = album.tracks_count {
                println!("   Tracks: {}", tracks_count);
            }
            if !album.genres.is_empty() {
                println!("   Genres: {}", album.genres.join(", "));
            }
        }
    } else {
        println!("⚠️  No favorite albums found.");
    }

    println!("\n✨ Example complete!");
    println!("\n💡 Tips:");
    println!("   - Run the example again to see faster loading from cache");
    println!("   - Check ./cache/qobuz-covers/ for cached cover images (WebP)");
    println!("   - Check ./cache/qobuz-audio/ for cached Hi-Res FLAC files");
    println!("   - Qobuz provides rich metadata (label, ISRC, sample rate, bit depth)");
    println!("   - Cached audio retains original quality (up to 24bit/192kHz)");

    Ok(())
}
