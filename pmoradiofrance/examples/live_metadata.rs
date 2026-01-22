//! Example: Get live metadata for Radio France stations
//!
//! Run with: cargo run -p pmoradiofrance --example live_metadata
//! Or with a specific station: cargo run -p pmoradiofrance --example live_metadata -- fip_rock

use pmoradiofrance::RadioFranceClient;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Get station from command line or use default
    let station = env::args()
        .nth(1)
        .unwrap_or_else(|| "franceculture".to_string());

    println!("Fetching live metadata for {}...\n", station);

    let client = RadioFranceClient::new().await?;
    let metadata = client.live_metadata(&station).await?;

    println!("Station: {}", metadata.station_name);
    println!("---");

    // Current show
    println!("Now playing:");
    println!("  Show: {}", metadata.now.first_line.title_or_default());
    println!("  Episode: {}", metadata.now.second_line.title_or_default());

    if let Some(producer) = &metadata.now.producer {
        println!("  Producer: {}", producer);
    }

    if let Some(intro) = &metadata.now.intro {
        let short_intro = if intro.len() > 100 {
            format!("{}...", &intro[..100])
        } else {
            intro.clone()
        };
        println!("  Description: {}", short_intro);
    }

    // Song info (for music stations)
    if let Some(song) = &metadata.now.song {
        println!("\nSong info:");
        println!("  Artist: {}", song.artists_display());
        if let Some(album) = &song.release.title {
            println!("  Album: {}", album);
        }
        if let Some(year) = song.year {
            println!("  Year: {}", year);
        }
        if let Some(label) = &song.release.label {
            println!("  Label: {}", label);
        }
    }

    // Timing
    println!("\nTiming:");
    if let Some(start) = metadata.now.start_time {
        let start_time = chrono::DateTime::from_timestamp(start as i64, 0)
            .map(|dt| dt.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "?".to_string());
        println!("  Started at: {}", start_time);
    }
    if let Some(end) = metadata.now.end_time {
        let end_time = chrono::DateTime::from_timestamp(end as i64, 0)
            .map(|dt| dt.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "?".to_string());
        println!("  Ends at: {}", end_time);
    }
    println!(
        "  Next refresh in: {} seconds",
        metadata.delay_to_refresh / 1000
    );

    // Streams
    println!("\nAvailable streams:");
    for source in &metadata.now.media.sources {
        println!(
            "  {:?} {} {} kbps: {}",
            source.broadcast_type,
            source.format.mime_type(),
            source.bitrate,
            source.url
        );
    }

    // Best HiFi stream
    if let Some(best) = metadata.now.media.best_hifi_stream() {
        println!("\nRecommended HiFi stream:");
        println!("  {}", best.url);
    }

    // Next show preview
    if let Some(next) = &metadata.next {
        println!("\nComing up next:");
        println!("  {}", next.first_line.title_or_default());
        if let Some(producer) = &next.producer {
            println!("  by {}", producer);
        }
    }

    Ok(())
}
