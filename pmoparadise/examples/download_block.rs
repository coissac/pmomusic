//! Télécharge un bloc complet de Radio Paradise et sauvegarde toutes les pistes en FLAC
//!
//! Ce programme démontre l'utilisation de la chaîne :
//! 1. RadioParadiseStreamSource - Télécharge et décode un bloc FLAC de Radio Paradise
//! 2. FlacFileSink - Sauvegarde automatiquement chaque piste dans un fichier FLAC séparé
//!
//! La nouvelle architecture AudioPipelineNode permet de :
//! - Télécharger et décoder automatiquement les blocs FLAC de Radio Paradise
//! - Détecter les limites de pistes (TrackBoundary)
//! - Sauvegarder automatiquement chaque piste dans un fichier séparé
//! - Gérer proprement l'arrêt du pipeline avec un CancellationToken
//!
//! Usage:
//!   cargo run --example download_block -- <channel_id>
//!
//! Exemple:
//!   cargo run --example download_block -- 0    # Main Mix
//!   cargo run --example download_block -- 1    # Mellow Mix
//!   cargo run --example download_block -- 2    # Rock Mix
//!   cargo run --example download_block -- 3    # World/Etc Mix

use pmoaudio::{AudioPipelineNode, FlacFileSink};
use pmoparadise::{RadioParadiseClient, RadioParadiseStreamSource};
use std::env;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialiser tracing pour le debug
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // Récupérer les arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <channel_id>", args[0]);
        eprintln!();
        eprintln!("Downloads a complete Radio Paradise block and saves all tracks as FLAC files.");
        eprintln!();
        eprintln!("Channel IDs:");
        eprintln!("  0 - Main Mix (eclectic, diverse mix)");
        eprintln!("  1 - Mellow Mix (smooth, chilled music)");
        eprintln!("  2 - Rock Mix (classic & modern rock)");
        eprintln!("  3 - World/Etc Mix (global sounds)");
        eprintln!();
        eprintln!("Example:");
        eprintln!("  {} 0    # Download Main Mix", args[0]);
        eprintln!("  {} 2    # Download Rock Mix", args[0]);
        std::process::exit(1);
    }

    let channel_id: u8 = match args[1].parse() {
        Ok(id) => id,
        Err(_) => {
            eprintln!("Error: channel_id must be a number between 0 and 3");
            std::process::exit(1);
        }
    };

    if channel_id > 3 {
        eprintln!("Error: channel_id must be between 0 and 3");
        std::process::exit(1);
    }

    println!("=== Radio Paradise Block Downloader ===");
    println!();
    println!("Channel ID: {}", channel_id);
    println!();

    // Créer le client Radio Paradise pour le channel spécifié
    println!("Fetching current block metadata...");
    let client = RadioParadiseClient::builder()
        .channel(channel_id)
        .build()
        .await?;

    // Récupérer le bloc actuel
    let block = client.get_block(None).await?;

    println!("Block Information:");
    println!("  Event ID: {}", block.event);
    println!("  Songs: {}", block.song_count());
    println!("  Duration: {:.1} minutes", block.length as f64 / 60000.0);
    println!();

    // Afficher la liste des pistes
    println!("Tracklist:");
    for (index, song) in block.songs_ordered() {
        println!(
            "  {:2}. {} - {} ({})",
            index + 1,
            song.artist,
            song.title,
            song.album.as_deref().unwrap_or("Unknown Album")
        );
    }
    println!();

    // Créer le répertoire de sortie
    let output_dir = format!("./rp_channel_{}block{}", channel_id, block.event);
    std::fs::create_dir_all(&output_dir)?;
    println!("Output directory: {}", output_dir);
    println!();

    // Créer le pipeline: RadioParadiseStreamSource → FlacFileSink
    let mut source = RadioParadiseStreamSource::new(client);

    // Ajouter le bloc à télécharger
    source.push_block_id(block.event);

    // Créer le sink qui sauvegarde chaque piste dans un fichier séparé
    let base_path = format!("{}/track.flac", output_dir);
    let sink = FlacFileSink::new(&base_path);

    // Construire la chaîne: source → sink
    source.register(Box::new(sink));

    // Créer un token d'arrêt
    let stop_token = CancellationToken::new();

    // Gérer Ctrl+C pour arrêt propre
    let stop_token_clone = stop_token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        println!("\n\nReceived Ctrl+C, stopping...");
        stop_token_clone.cancel();
    });

    // Lancer tout le pipeline
    println!("Downloading and processing block...");
    println!("Press Ctrl+C to stop.");
    println!();
    let start = std::time::Instant::now();

    let result = Box::new(source).run(stop_token).await;

    let elapsed = start.elapsed();

    // Vérifier le résultat
    match result {
        Ok(()) => {
            println!();
            println!(
                "✓ Download completed successfully in {:.2}s",
                elapsed.as_secs_f64()
            );
            println!("  Output directory: {}", output_dir);
            println!();

            // Afficher les fichiers créés
            let entries = std::fs::read_dir(&output_dir)?;
            let mut files: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(|s| s == "flac")
                        .unwrap_or(false)
                })
                .collect();
            files.sort_by_key(|e| e.path());

            println!("Files created:");
            for (i, entry) in files.iter().enumerate() {
                let path = entry.path();
                let metadata = std::fs::metadata(&path)?;
                let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
                println!(
                    "  {:2}. {} ({:.2} MB)",
                    i + 1,
                    path.file_name().unwrap().to_string_lossy(),
                    size_mb
                );
            }
            println!();

            // Calculer la taille totale
            let total_size: u64 = files
                .iter()
                .filter_map(|e| std::fs::metadata(e.path()).ok())
                .map(|m| m.len())
                .sum();
            println!(
                "Total size: {:.2} MB",
                total_size as f64 / (1024.0 * 1024.0)
            );
        }
        Err(e) => {
            eprintln!();
            eprintln!("✗ Download error: {}", e);
            eprintln!();
            return Err(e.into());
        }
    }

    Ok(())
}
