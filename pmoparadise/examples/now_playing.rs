//! Example: Display currently playing song and block information
//!
//! This example demonstrates:
//! - Creating a Radio Paradise client
//! - Fetching the current block
//! - Displaying song metadata
//! - Generating cover image URLs
//!
//! Run with: cargo run --example now_playing

use pmoparadise::{RadioParadiseClient, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging (optional)
    #[cfg(feature = "logging")]
    tracing_subscriber::fmt::init();

    println!("Radio Paradise - Now Playing");
    println!("=============================\n");

    // Create client with default settings (FLAC quality, channel 0)
    let client = RadioParadiseClient::new().await?;

    // Get what's currently playing
    let now_playing = client.now_playing().await?;
    let block = &now_playing.block;

    // Display block information
    println!("Block Information:");
    println!("  Event ID: {}", block.event);
    println!("  Next Event: {}", block.end_event);
    println!("  Duration: {:.1} minutes", block.length as f64 / 60000.0);
    println!("  Songs in block: {}", block.song_count());
    println!("  Stream URL: {}\n", block.url);

    // Display current song (if available)
    if let Some(song) = &now_playing.current_song {
        println!("Now Playing:");
        println!("  Title: {}", song.title);
        println!("  Artist: {}", song.artist);
        println!("  Album: {}", song.album);
        if let Some(year) = song.year {
            println!("  Year: {}", year);
        }
        if let Some(rating) = song.rating {
            println!("  Rating: {:.1}/10", rating);
        }
        println!("  Duration: {}:{:02}",
                 song.duration / 60000,
                 (song.duration % 60000) / 1000);

        // Display cover URL
        if let Some(cover) = &song.cover {
            if let Some(cover_url) = block.cover_url(cover) {
                println!("  Cover: {}", cover_url);
            }
        }
        println!();
    }

    // Display all songs in the block
    println!("All Songs in This Block:");
    println!("------------------------");

    for (index, song) in block.songs_ordered() {
        let start_sec = song.elapsed / 1000;
        let duration_sec = song.duration / 1000;

        println!(
            "{}. [{:02}:{:02}] {} - {} ({:02}:{:02})",
            index + 1,
            start_sec / 60,
            start_sec % 60,
            song.artist,
            song.title,
            duration_sec / 60,
            duration_sec % 60
        );
        println!("   Album: {}", song.album);

        if let Some(year) = song.year {
            print!("   Year: {}", year);
        }
        if let Some(rating) = song.rating {
            print!("   Rating: {:.1}/10", rating);
        }
        println!("\n");
    }

    // Show how to get the next block
    println!("Fetching Next Block...");
    let next_block = client.get_block(Some(block.end_event)).await?;
    println!("  Next block event: {}", next_block.event);
    println!("  Songs in next block: {}", next_block.song_count());

    if let Some((_, first_song)) = next_block.songs_ordered().first() {
        println!("  First song: {} - {}", first_song.artist, first_song.title);
    }

    Ok(())
}
