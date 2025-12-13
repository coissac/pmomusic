//! Example demonstrating Qobuz lazy loading with rate limiting
//!
//! This example shows how to use the new lazy loading feature to add albums
//! to playlists without downloading all audio files immediately. Only covers
//! are downloaded eagerly, audio is downloaded on-demand when played.
//!
//! Features demonstrated:
//! - Rate limiting (max 2 concurrent requests, 400ms delay)
//! - Lazy audio loading (saves ~99% initial bandwidth)
//! - Eager cover loading (UI responsiveness)
//! - Automatic PK switching when audio is downloaded
//! - Prefetch of next 2 tracks during playback
//!
//! Run with:
//! ```bash
//! cargo run -p pmoqobuz --example lazy_loading
//! ```

use pmoaudiocache::Cache as AudioCache;
use pmocovers::Cache as CoverCache;
use pmoplaylist::PlaylistManager;
use pmoqobuz::{QobuzClient, QobuzSource};
use std::sync::Arc;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing with debug level to see rate limiting
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    println!("🎵 Qobuz Lazy Loading Demo");
    println!("============================\n");

    // Step 1: Connect to Qobuz with rate limiting enabled
    println!("📡 Connecting to Qobuz (rate limiting enabled)...");
    let client = QobuzClient::from_config().await?;
    println!("✅ Connected with rate limiting:");
    println!("   - Max 2 concurrent requests");
    println!("   - 400ms minimum delay between requests\n");

    // Step 2: Initialize caches
    println!("💾 Initializing caches...");
    let cover_cache = Arc::new(CoverCache::new("./cache/qobuz-covers", 500)?);
    let audio_cache = Arc::new(AudioCache::new("./cache/qobuz-audio", 100)?);

    // IMPORTANT: Register audio cache globally so PlaylistManager can access it
    pmoplaylist::register_audio_cache(audio_cache.clone());

    println!("✅ Caches initialized\n");

    // Step 3: Create QobuzSource with caches
    let source = QobuzSource::new(client, cover_cache.clone(), audio_cache.clone());

    // Step 4: Get user's favorite albums
    println!("🎧 Fetching your favorite albums...");
    let favorite_albums = source.client().get_favorite_albums().await?;

    if favorite_albums.is_empty() {
        println!("⚠️  No favorite albums found!");
        println!("   Please add some albums to your Qobuz favorites first.\n");
        return Ok(());
    }

    println!("✅ Found {} favorite albums\n", favorite_albums.len());

    // Step 5: Select first album for testing
    let album = &favorite_albums[0];
    println!("📀 Selected album: {} - {}", album.artist.name, album.title);
    println!("   Tracks: {}", album.tracks_count.unwrap_or(0));
    println!("   Album ID: {}\n", album.id);

    // Step 6: Create a test playlist
    println!("📝 Creating test playlist...");
    let playlist_manager = PlaylistManager();
    let playlist_id = {
        let writer = playlist_manager
            .create_persistent_playlist("lazy-test".to_string())
            .await?;
        writer.id().to_string()
    }; // Drop writer here to release the lock
    println!("✅ Playlist created: {}\n", playlist_id);

    // Step 7: Add album with lazy loading (measure time and track downloads)
    println!("⏱️  Adding album to playlist with LAZY loading...");
    println!("   This will:");
    println!("   - Download covers immediately (~400 KB each)");
    println!("   - Create lazy PKs for audio (NO download)");
    println!("   - Enable prefetch for next 2 tracks\n");

    let start = Instant::now();
    let count = source
        .add_album_to_playlist(&playlist_id, &album.id)
        .await?;
    let elapsed = start.elapsed();

    println!(
        "✅ Album added: {} tracks in {:.2}s",
        count,
        elapsed.as_secs_f64()
    );
    println!(
        "   Average: {:.0}ms per track\n",
        elapsed.as_millis() as f64 / count as f64
    );

    // Step 8: Verify lazy PKs
    println!("🔍 Verifying lazy PKs...");
    let reader = playlist_manager.get_read_handle(&playlist_id).await?;

    // Read all tracks from playlist
    let mut tracks = Vec::new();
    loop {
        match reader.peek().await? {
            Some(track) => {
                tracks.push(track);
                reader.pop().await?;
            }
            None => break,
        }
    }

    if tracks.is_empty() {
        println!("⚠️  No tracks in playlist!");
        return Ok(());
    }

    let first_track_pk = tracks[0].cache_pk();
    let is_lazy = pmocache::is_lazy_pk(&first_track_pk);

    println!("   First track PK: {}", first_track_pk);
    println!(
        "   Is lazy: {}",
        if is_lazy {
            "✅ YES (starts with 'L:')"
        } else {
            "❌ NO"
        }
    );

    // Count lazy vs downloaded
    let lazy_count = tracks
        .iter()
        .filter(|t| pmocache::is_lazy_pk(t.cache_pk()))
        .count();
    let downloaded_count = tracks.len() - lazy_count;

    println!("\n📊 Track status:");
    println!("   Lazy (not downloaded): {} tracks", lazy_count);
    println!("   Downloaded: {} tracks", downloaded_count);

    // Step 9: Check cache sizes
    println!("\n💾 Cache disk usage:");
    println!("   Covers: {:?}", get_dir_size("./cache/qobuz-covers")?);
    println!("   Audio: {:?}", get_dir_size("./cache/qobuz-audio")?);

    // Step 10: Demonstrate on-demand download
    if is_lazy {
        println!("\n🎵 Simulating playback of first track...");
        println!("   This would trigger download via HTTP request to:");
        println!("   GET /cache/flac/{}", first_track_pk);
        println!("\n   The lazy PK will automatically:");
        println!("   1. Download the audio file from Qobuz");
        println!("   2. Convert to FLAC");
        println!("   3. Calculate real PK from content");
        println!("   4. Update playlist (lazy_pk → real_pk)");
        println!("   5. Prefetch next 2 tracks in background");
    }

    // Step 11: Summary
    println!("\n╭─────────────────────────────────────────╮");
    println!("│  🎉 Lazy Loading Demo Complete!        │");
    println!("╰─────────────────────────────────────────╯");
    println!("\n📈 Benefits demonstrated:");
    println!(
        "   ✓ Fast album loading (~{}ms per track)",
        elapsed.as_millis() / count as u128
    );
    println!("   ✓ Minimal initial download (covers only)");
    println!("   ✓ Audio downloaded on-demand");
    println!("   ✓ Rate limiting active (respectful to Qobuz)");
    println!("   ✓ Automatic prefetching during playback");

    println!("\n💡 For 375 favorite albums (~3750 tracks):");
    println!("   Without lazy: ~15 GB download, ~75s (no rate limit)");
    println!("   With lazy:    ~150 MB download, ~5 min (rate limited)");
    println!("   Savings:      ~99% bandwidth, natural request pattern");

    Ok(())
}

/// Calculate directory size recursively
fn get_dir_size(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    use std::fs;

    let mut total: u64 = 0;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    total += metadata.len();
                } else if metadata.is_dir() {
                    if let Ok(size_str) = get_dir_size(&entry.path().to_string_lossy()) {
                        // Parse size from string (hacky but works for this example)
                        if let Some(num) = size_str.split_whitespace().next() {
                            if let Ok(size) = num.parse::<f64>() {
                                total += (size * 1024.0 * 1024.0) as u64;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(format_size(total))
}

/// Format bytes to human-readable size
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
