//! Convertisseur de fichiers audio vers FLAC 24-bit
//!
//! Ce programme démontre l'utilisation de la chaîne :
//! 1. FileSource - Lecture d'un fichier audio (FLAC, MP3, OGG, WAV, AIFF)
//! 2. ToI24Node - Conversion vers 24-bit signed integer
//! 3. FlacFileSink - Écriture au format FLAC
//!
//! La nouvelle architecture AudioPipelineNode permet de :
//! - Construire le pipeline en enregistrant des enfants avec register()
//! - Insérer des nœuds de conversion de type pour garantir la profondeur de bit souhaitée
//! - Lancer tout le pipeline avec un seul appel à run() sur la racine
//! - Arrêter proprement tout le pipeline avec un CancellationToken
//!
//! Usage:
//!   cargo run --example convert_to_flac24 -- <input_file> <output_file>
//!
//! Exemple:
//!   cargo run --example convert_to_flac24 -- input.mp3 output.flac
//!   cargo run --example convert_to_flac24 -- input16bit.flac output24bit.flac

use pmoaudio::{AudioPipelineNode, FileSource, FlacFileSink, ToI24Node};
use std::env;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialiser tracing pour le debug
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Récupérer les arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input_file> <output_file>", args[0]);
        eprintln!();
        eprintln!("Converts any audio file to FLAC with 24-bit depth.");
        eprintln!();
        eprintln!("Supported input formats:");
        eprintln!("  - FLAC (8/16/24/32-bit)");
        eprintln!("  - MP3");
        eprintln!("  - OGG Vorbis");
        eprintln!("  - WAV");
        eprintln!("  - AIFF");
        eprintln!();
        eprintln!("Example:");
        eprintln!("  {} input.mp3 output.flac", args[0]);
        eprintln!("  {} input16bit.flac output24bit.flac", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];

    println!("=== Audio to FLAC 24-bit Converter ===");
    println!();
    println!("Input:  {}", input_path);
    println!("Output: {}", output_path);
    println!();
    println!("Pipeline: FileSource → ToI24Node → FlacFileSink");
    println!();

    // Créer le pipeline: FileSource → ToI24Node → FlacFileSink
    let mut source = FileSource::new(input_path);

    // Le ToI24Node convertit tous les chunks audio en 24-bit
    // Cela garantit que le FlacFileSink encodera en 24-bit
    let mut converter = ToI24Node::new();

    let sink = FlacFileSink::new(output_path);

    // Construire la chaîne: source → converter → sink
    converter.register(Box::new(sink));
    source.register(converter);

    // Créer un token d'arrêt pour contrôle manuel si besoin
    let stop_token = CancellationToken::new();

    // Lancer tout le pipeline - run() spawne automatiquement tous les enfants
    println!("Processing...");
    let start = std::time::Instant::now();

    let result = Box::new(source).run(stop_token).await;

    let elapsed = start.elapsed();

    // Vérifier le résultat
    match result {
        Ok(()) => {
            println!();
            println!("✓ Conversion completed successfully in {:.2}s", elapsed.as_secs_f64());
            println!("  Output file: {}", output_path);
            println!();

            // Afficher des informations supplémentaires si possible
            if let Ok(metadata) = std::fs::metadata(output_path) {
                let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
                println!("  File size: {:.2} MB", size_mb);
            }
        }
        Err(e) => {
            eprintln!();
            eprintln!("✗ Conversion error: {}", e);
            eprintln!();
            return Err(e.into());
        }
    }

    Ok(())
}
