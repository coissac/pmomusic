//! Test simplifié - va directement à get_file_url
//! Pour comparaison avec Python via fake server
//!
//! Ce test est conçu pour être utilisé avec le fake_qobuz_server.py
//! Les credentials sont hardcodés car le fake server les accepte tous

use anyhow::Result;
use pmoqobuz::api::{QobuzApi, Spoofer};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialiser le logging
    tracing_subscriber::fmt::init();

    println!("=== Test get_file_url (Rust) ===\n");

    // Track ID connu (récupéré du test Python)
    let track_id = "19557883";
    let format_id = 27;

    println!("1. Creating QobuzApi (auto-initializes Spoofer)...");
    let spoofer = Spoofer::new().await?;
    let app_id = spoofer.get_app_id()?;
    let app_secret = spoofer.get_app_secret()?;

    let mut api = QobuzApi::with_raw_secret(&app_id, &app_secret)?;

    // Configurer format_id = 27 (comme Python)
    use pmoqobuz::AudioFormat;
    api.set_format(AudioFormat::Flac_HiRes_192);

    println!("   ✓ API created");
    println!("   App ID: {}", app_id);
    println!("   Format: 27 (Flac_HiRes_192) - same as Python");

    println!("\n2. Logging in...");
    println!("   Using fake credentials (fake server accepts all)");
    let username = "eric@coissac.eu";
    let password = "fake_password";

    let user = api.login(username, password).await?;
    println!("   ✓ Login successful - User ID: {}", user.user_id);
    println!("   Token: {}...", &user.token[..20.min(user.token.len())]);

    println!("\n3. Calling track/getFileUrl...");
    println!("   Track ID: {}", track_id);
    println!("   Format ID: {} (default)", format_id);
    println!("   ⚠️  THIS CALL IS SIGNED - Watch fake server logs!\n");

    match api.get_file_url(track_id).await {
        Ok(stream_info) => {
            println!("   ✓ Success!");
            println!(
                "   URL: {}...",
                &stream_info.url[..80.min(stream_info.url.len())]
            );
            println!("   MIME type: {}", stream_info.mime_type);
        }
        Err(e) => {
            println!("   ✗ Failed: {}", e);
            return Err(e.into());
        }
    }

    println!("\n=== Test completed successfully! ===");
    Ok(())
}
