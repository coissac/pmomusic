//! Exemple de test pour la conversion FLAC en streaming
//!
//! Cet exemple télécharge un fichier audio depuis une URL et le convertit
//! en FLAC en utilisant la fonction create_flac_transformer().

use pmoaudiocache::cache;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialiser le logger
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Créer un répertoire temporaire pour le cache
    let cache_dir = "/tmp/test_audio_cache";
    std::fs::create_dir_all(cache_dir)?;

    println!("Création du cache audio avec conversion FLAC streaming...");
    let cache = Arc::new(cache::new_cache(cache_dir, 100)?);

    // URL de test - fichier audio de test public
    // Note: Remplacez par une URL valide de votre choix
    let test_url = "https://www.soundhelix.com/examples/mp3/SoundHelix-Song-1.mp3";

    println!("Téléchargement et conversion de: {}", test_url);
    println!("Ceci va télécharger le fichier en streaming et le convertir en FLAC...");

    // Ajouter le fichier au cache avec conversion FLAC
    match cache::add_with_metadata_extraction(&cache, test_url, Some("test:streaming")).await {
        Ok(pk) => {
            println!("✓ Fichier converti avec succès!");
            println!("  Clé primaire: {}", pk);

            let file_path = cache.file_path(&pk);
            println!("  Chemin: {}", file_path.display());

            if let Ok(metadata) = std::fs::metadata(&file_path) {
                println!("  Taille: {} bytes", metadata.len());
            }

            // Récupérer les métadonnées audio
            match cache::get_metadata(&cache, &pk) {
                Ok(metadata) => {
                    println!("  Métadonnées:");
                    if let Some(title) = &metadata.title {
                        println!("    Titre: {}", title);
                    }
                    if let Some(artist) = &metadata.artist {
                        println!("    Artiste: {}", artist);
                    }
                    if let Some(duration) = metadata.duration_secs {
                        println!("    Durée: {}s", duration);
                    }
                }
                Err(e) => println!("  Impossible de lire les métadonnées: {}", e),
            }
        }
        Err(e) => {
            eprintln!("✗ Erreur lors de la conversion: {}", e);
            return Err(e);
        }
    }

    println!("\nTest terminé avec succès!");

    Ok(())
}
