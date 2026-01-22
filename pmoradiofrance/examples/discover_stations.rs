//! Example: Discover all Radio France stations
//!
//! Run with: cargo run -p pmoradiofrance --example discover_stations

use pmoradiofrance::RadioFranceClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("Discovering Radio France stations...\n");

    let client = RadioFranceClient::new().await?;
    let stations = client.discover_all_stations().await?;

    // Count by type
    let main_count = stations.iter().filter(|s| s.is_main()).count();
    let webradio_count = stations.iter().filter(|s| s.is_webradio()).count();
    let local_count = stations.iter().filter(|s| s.is_local_radio()).count();

    println!("Found {} stations total:\n", stations.len());

    println!("=== Main Stations ({}) ===", main_count);
    for station in stations.iter().filter(|s| s.is_main()) {
        println!("  {} ({})", station.name, station.slug);
    }

    println!("\n=== Webradios ({}) ===", webradio_count);
    for station in stations.iter().filter(|s| s.is_webradio()) {
        println!("  {} ({})", station.name, station.slug);
    }

    println!("\n=== Local Radios ({}) ===", local_count);
    for station in stations.iter().filter(|s| s.is_local_radio()) {
        println!("  {} ({})", station.name, station.slug);
    }

    Ok(())
}
