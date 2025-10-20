//! Example: Stream a Radio Paradise block with prefetching
//!
//! This example demonstrates:
//! - Streaming block audio data
//! - Writing to a file or piping to a player
//! - Prefetching the next block for gapless playback
//! - Continuous playback loop
//!
//! Run with: cargo run --example stream_block
//!
//! To play directly with mpv:
//!   cargo run --example stream_block | mpv --no-cache --demuxer=+lavf -

use futures::StreamExt;
use pmoparadise::{RadioParadiseClient, Result};
use std::io::Write;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging (optional)
    #[cfg(feature = "logging")]
    tracing_subscriber::fmt::init();

    eprintln!("Radio Paradise - Block Streaming Demo");
    eprintln!("======================================\n");

    // Create client
    let mut client = RadioParadiseClient::builder()
        .bitrate(pmoparadise::Bitrate::Flac)
        .build()
        .await?;

    eprintln!("Client configured for FLAC streaming\n");

    // Get current block
    let current_block = client.get_block(None).await?;

    eprintln!("Current Block:");
    eprintln!("  Event: {}", current_block.event);
    eprintln!("  Songs: {}", current_block.song_count());
    eprintln!(
        "  Duration: {:.1} minutes",
        current_block.length as f64 / 60000.0
    );
    eprintln!("  URL: {}\n", current_block.url);

    // Display tracklist
    eprintln!("Tracklist:");
    for (index, song) in current_block.songs_ordered() {
        eprintln!("  {}. {} - {}", index + 1, song.artist, song.title);
    }
    eprintln!();

    // Prefetch next block in advance
    eprintln!("Prefetching next block...");
    client.prefetch_next(&current_block).await?;
    eprintln!(
        "Next block prefetched: {}\n",
        client.next_block_url().unwrap()
    );

    // Stream the block
    eprintln!("Streaming block... (writing to stdout)");
    eprintln!("Tip: Pipe to a player like: cargo run --example stream_block | mpv -\n");

    let mut stream = client.stream_block_from_metadata(&current_block).await?;
    let mut total_bytes = 0u64;
    let mut stdout = std::io::stdout();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        total_bytes += chunk.len() as u64;

        // Write to stdout (can be piped to a player)
        stdout.write_all(&chunk)?;
        stdout.flush()?;

        // Progress indicator (to stderr so it doesn't interfere with piped audio)
        if total_bytes % (1024 * 1024) == 0 {
            eprintln!(
                "  Downloaded: {:.1} MB",
                total_bytes as f64 / 1024.0 / 1024.0
            );
        }
    }

    eprintln!("\nBlock streaming complete!");
    eprintln!(
        "Total downloaded: {:.2} MB",
        total_bytes as f64 / 1024.0 / 1024.0
    );

    // In a real application, you would now:
    // 1. Get the next block using prefetched metadata
    // 2. Stream it seamlessly
    // 3. Prefetch the following block
    // 4. Repeat for continuous playback

    eprintln!("\nFor continuous playback, you would now stream the next block:");
    eprintln!("  Event: {}", current_block.end_event);

    Ok(())
}
