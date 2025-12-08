//! Exemple d'utilisation du trait QobuzConfigExt
//!
//! Cet exemple montre comment utiliser le trait d'extension pour gérer
//! les credentials Qobuz via pmoconfig.
//!
//! Usage:
//! ```bash
//! cargo run --example config_usage
//! ```

use pmoconfig::get_config;
use pmoqobuz::QobuzConfigExt;

fn main() -> anyhow::Result<()> {
    // Initialiser le logging
    tracing_subscriber::fmt::init();

    println!("=== QobuzConfigExt Example ===\n");

    // Récupérer la configuration globale
    let config = get_config();

    // Exemple 1: Lire les credentials existants
    println!("--- Lecture des credentials ---");
    match config.get_qobuz_credentials() {
        Ok((username, password)) => {
            println!("Username: {}", username);
            println!("Password: {}", "*".repeat(password.len()));
        }
        Err(e) => {
            println!("Credentials non configurés: {}", e);
        }
    }

    // Exemple 2: Lire username et password séparément
    println!("\n--- Lecture séparée ---");
    match config.get_qobuz_username() {
        Ok(username) => println!("Username: {}", username),
        Err(e) => println!("Username non configuré: {}", e),
    }

    match config.get_qobuz_password() {
        Ok(password) => println!("Password: {}", "*".repeat(password.len())),
        Err(e) => println!("Password non configuré: {}", e),
    }

    // Exemple 3: Définir de nouveaux credentials (commenté pour ne pas modifier la config)
    /*
    println!("\n--- Définition de nouveaux credentials ---");
    config.set_qobuz_username("user@example.com")?;
    config.set_qobuz_password("my_secure_password")?;
    println!("Nouveaux credentials enregistrés !");
    */

    // Exemple 4: Utilisation avec QobuzClient
    println!("\n--- Utilisation avec QobuzClient ---");
    println!("Pour créer un client Qobuz à partir de la config:");
    println!("  let client = QobuzClient::from_config().await?;");
    println!("\nCette méthode utilise automatiquement QobuzConfigExt");
    println!("pour récupérer les credentials depuis pmoconfig.");

    Ok(())
}
