//! Example: Extract individual tracks from a FLAC block (requires `per-track` feature)
//!
//! This example demonstrates:
//! - Per-track extraction from FLAC blocks
//! - Exporting tracks to WAV files
//! - Alternative player-based seeking (recommended)
//!
//! **Warning**: This approach downloads and decodes entire blocks.
//! For most use cases, player-based seeking is more efficient.
//!
//! Run with: cargo run --example extract_track --features per-track

#[cfg(feature = "per-track")]
use pmoparadise::{RadioParadiseClient, Result};
#[cfg(feature = "per-track")]
use std::path::Path;

#[cfg(feature = "per-track")]
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    #[cfg(feature = "logging")]
    tracing_subscriber::fmt::init();

    println!("Radio Paradise - Per-Track Extraction Demo");
    println!("===========================================\n");

    println!("WARNING: This feature downloads entire blocks (50-100MB)");
    println!("         and performs CPU-intensive FLAC decoding.");
    println!("         For most use cases, player-based seeking is better.\n");

    // Create client
    let client = RadioParadiseClient::new().await?;

    // Get current block
    let block = client.get_block(None).await?;

    println!("Block Information:");
    println!("  Event: {}", block.event);
    println!("  Songs: {}", block.song_count());
    println!("  URL: {}\n", block.url);

    // Display all tracks
    println!("Available Tracks:");
    for (index, song) in block.songs_ordered() {
        println!("  {}. {} - {} ({:.1}s)",
                 index,
                 song.artist,
                 song.title,
                 song.duration as f64 / 1000.0);
    }
    println!();

    // Extract first track
    let track_index = 0;
    if let Some((_, song)) = block.songs_ordered().first() {
        println!("Extracting Track {}:", track_index);
        println!("  Artist: {}", song.artist);
        println!("  Title: {}", song.title);
        println!("  Album: {}\n", song.album);

        println!("Downloading and decoding... (this may take a while)");

        // Open track stream
        let mut track_stream = client.open_track_stream(&block, track_index).await?;

        println!("Track Metadata:");
        println!("  Sample Rate: {} Hz", track_stream.metadata.sample_rate);
        println!("  Channels: {}", track_stream.metadata.channels);
        println!("  Bits Per Sample: {}", track_stream.metadata.bits_per_sample);
        println!("  Total Samples: {}", track_stream.metadata.total_samples);
        println!();

        // Export to WAV
        let output_path = Path::new("track.wav");
        println!("Exporting to {:?}...", output_path);
        track_stream.export_wav(output_path)?;
        println!("✓ Export complete!\n");
    }

    // Show alternative: player-based seeking
    println!("RECOMMENDED ALTERNATIVE: Player-Based Seeking");
    println!("=============================================\n");

    for (index, song) in block.songs_ordered().into_iter().take(3) {
        let (start, duration) = client.track_position_seconds(&block, index)?;
        println!("Track {}: {} - {}", index, song.artist, song.title);
        println!("  mpv command:");
        println!("    mpv --start={:.3} --length={:.3} '{}'", start, duration, block.url);
        println!("  ffmpeg command (extract to file):");
        println!("    ffmpeg -ss {:.3} -t {:.3} -i '{}' -c copy track_{}.flac",
                 start, duration, block.url, index);
        println!();
    }

    println!("These methods are much more efficient as they:");
    println!("  - Don't download the entire block");
    println!("  - Use the player's optimized seeking");
    println!("  - Start playback immediately");
    println!("  - Preserve original quality (with -c copy)");

    Ok(())
}

#[cfg(not(feature = "per-track"))]
fn main() {
    eprintln!("ERROR: This example requires the 'per-track' feature.");
    eprintln!("Run with: cargo run --example extract_track --features per-track");
    std::process::exit(1);
}
