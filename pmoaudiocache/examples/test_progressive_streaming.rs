//! Test du streaming progressif avec le nouveau transformer
//!
//! Cet exemple démontre comment les fichiers deviennent disponibles
//! progressivement pendant le téléchargement avec le nouveau système.

use pmoaudiocache::cache;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialiser le logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("=== Test du cache audio avec streaming progressif ===\n");

    // Créer un cache temporaire
    let cache_dir = "/tmp/test_streaming_cache";
    let _ = std::fs::remove_dir_all(cache_dir);
    let cache = cache::new_cache(cache_dir, 10)?;

    println!("Cache créé dans: {}\n", cache_dir);

    // URL d'un fichier FLAC pour tester le streaming complet
    // Pour tester, vous pouvez utiliser votre propre URL ou un fichier local
    let test_url = std::env::var("TEST_AUDIO_URL")
        .unwrap_or_else(|_| "https://www.kozco.com/tech/piano2-CoolEdit.flac".to_string());

    println!("Test avec URL: {}\n", test_url);

    // Démarrer le téléchargement et la conversion
    println!("🚀 Démarrage du téléchargement et de la conversion...");
    let start = Instant::now();

    // Ajouter avec extraction de métadonnées
    let pk = cache::add_with_metadata_extraction(&cache, &test_url, None).await?;

    let total_time = start.elapsed();
    println!("   ✓ Ajouté au cache avec pk: {}", pk);
    println!("   ✓ Temps total: {:?}", total_time);

    // Vérifier que le fichier est bien accessible
    println!("\n🔍 Vérification du fichier:");
    let file_path = cache.get(&pk).await?;
    let file_size = tokio::fs::metadata(&file_path).await?.len();
    println!("   • Chemin: {:?}", file_path);
    println!("   • Taille: {} bytes", file_size);

    // Extraire et afficher les métadonnées
    println!("\n📋 Métadonnées extraites:");
    match cache::get_metadata(&cache, &pk) {
        Ok(metadata) => {
            println!("   • Titre: {:?}", metadata.title);
            println!("   • Artiste: {:?}", metadata.artist);
            println!("   • Album: {:?}", metadata.album);
            println!("   • Durée: {:?} secondes", metadata.duration_secs);
            println!("   • Sample rate: {:?} Hz", metadata.sample_rate);
            println!("   • Channels: {:?}", metadata.channels);
        }
        Err(e) => {
            println!("   ⚠️  Métadonnées non disponibles: {}", e);
        }
    }

    println!("\n✨ Test terminé avec succès !");

    Ok(())
}
