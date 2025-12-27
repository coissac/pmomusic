//! Simple Chromecast info retrieval test
//!
//! This is the most minimal test - just connects and gets device status.
//!
//! Usage:
//!   cargo run --example chromecast_info -- <chromecast_ip>

use std::env;
use std::sync::Once;

use rust_cast::CastDevice;

const DEFAULT_DESTINATION_ID: &str = "receiver-0";
const DEFAULT_PORT: u16 = 8009;

/// Ensures the Rustls CryptoProvider is initialized exactly once.
fn ensure_crypto_provider_initialized() {
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        let _ = rustls::crypto::CryptoProvider::install_default(
            rustls::crypto::aws_lc_rs::default_provider()
        );
    });
}

fn main() {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <chromecast_ip>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  {} 192.168.1.100", args[0]);
        std::process::exit(1);
    }

    let chromecast_ip = &args[1];

    println!("═══════════════════════════════════════════════════════");
    println!("  Chromecast Info Test");
    println!("═══════════════════════════════════════════════════════");
    println!();

    // Initialize crypto provider
    ensure_crypto_provider_initialized();

    // Connect to the device
    println!("→ Connecting to {}:{}...", chromecast_ip, DEFAULT_PORT);
    let cast_device = match CastDevice::connect_without_host_verification(chromecast_ip, DEFAULT_PORT) {
        Ok(device) => {
            println!("  ✓ Connected");
            device
        }
        Err(e) => {
            eprintln!("  ✗ Failed: {}", e);
            std::process::exit(1);
        }
    };

    // Connect to receiver channel
    println!();
    println!("→ Connecting to receiver channel...");
    if let Err(e) = cast_device.connection.connect(DEFAULT_DESTINATION_ID.to_string()) {
        eprintln!("  ✗ Failed: {}", e);
        std::process::exit(1);
    }
    println!("  ✓ Channel connected");

    // Send initial ping
    println!();
    println!("→ Sending initial ping...");
    if let Err(e) = cast_device.heartbeat.ping() {
        eprintln!("  ✗ Failed: {}", e);
        std::process::exit(1);
    }
    println!("  ✓ Ping sent");

    // Get receiver status
    println!();
    println!("→ Getting receiver status...");
    match cast_device.receiver.get_status() {
        Ok(status) => {
            println!("  ✓ Status retrieved\n");

            // Volume
            if let Some(level) = status.volume.level {
                println!("  Volume:  {:.0}%", level * 100.0);
            }
            if let Some(muted) = status.volume.muted {
                println!("  Muted:   {}", muted);
            }

            // Applications
            println!("\n  Running applications: {}", status.applications.len());
            for (i, app) in status.applications.iter().enumerate() {
                println!("\n  App #{}:", i + 1);
                println!("    Display Name:  {}", app.display_name);
                println!("    App ID:        {}", app.app_id);
                println!("    Session ID:    {}", app.session_id);
                println!("    Transport ID:  {}", app.transport_id);
                println!("    Status:        {}", app.status_text);
                println!("    Namespaces:    {}", app.namespaces.len());
            }

            if status.applications.is_empty() {
                println!("    (no apps currently running)");
            }
        }
        Err(e) => {
            eprintln!("  ✗ Failed: {}", e);
            std::process::exit(1);
        }
    }

    println!();
    println!("═══════════════════════════════════════════════════════");
    println!("  Test completed successfully!");
    println!("═══════════════════════════════════════════════════════");
}
