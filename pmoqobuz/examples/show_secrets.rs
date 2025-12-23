//! Affiche tous les secrets du Spoofer Rust

use anyhow::Result;
use pmoqobuz::api::Spoofer;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Rust Spoofer Secrets ===\n");

    let spoofer = Spoofer::new().await?;

    // App ID
    let app_id = spoofer.get_app_id()?;
    println!("App ID: {}\n", app_id);

    // App Secret (MD5 hash from bundle)
    match spoofer.get_app_secret() {
        Ok(app_secret) => {
            println!("App Secret (from bundle.js):");
            println!("  Full value: {}", app_secret);
            println!("  Length: {}\n", app_secret.len());
        }
        Err(e) => {
            println!("App Secret: Error - {}\n", e);
        }
    }

    // Timezone secrets
    match spoofer.get_secrets() {
        Ok(secrets) => {
            println!("Timezone Secrets:");
            println!("  Number of secrets: {}\n", secrets.len());

            for (i, (tz, secret)) in secrets.iter().enumerate() {
                println!("Secret {} (timezone: {}):", i + 1, tz);
                println!("  Full value: {}", secret);
                println!("  Length: {}", secret.len());
                println!();
            }
        }
        Err(e) => {
            println!("Timezone Secrets: Error - {}\n", e);
        }
    }

    Ok(())
}
