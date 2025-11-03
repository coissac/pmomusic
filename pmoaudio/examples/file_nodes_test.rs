//! Test d'intégration pour FileSource et FlacFileSink avec la nouvelle architecture AudioPipelineNode
//!
//! Ce programme teste la chaîne complète :
//! 1. Lecture d'un fichier audio avec FileSource
//! 2. Écriture vers FLAC avec FlacFileSink
//!
//! La nouvelle architecture permet de :
//! - Construire le pipeline en enregistrant des enfants avec register()
//! - Lancer tout le pipeline avec un seul appel à run() sur la racine
//! - Arrêter proprement tout le pipeline avec un CancellationToken
//!
//! Usage:
//!   cargo run --example file_nodes_test -- <input_file> <output_file>

use pmoaudio::{AudioPipelineNode, FileSource, FlacFileSink};
use std::env;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Récupérer les arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input_file> <output_file>", args[0]);
        eprintln!("Example: {} input.flac output.flac", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];

    println!("Input:  {}", input_path);
    println!("Output: {}", output_path);
    println!();

    // Créer le pipeline: FileSource → FlacFileSink
    let mut source = FileSource::new(input_path);
    let sink = FlacFileSink::new(output_path);

    // Enregistrer le sink comme enfant de la source
    source.register(Box::new(sink));

    // Créer un token d'arrêt pour contrôle manuel si besoin
    let stop_token = CancellationToken::new();

    // Lancer tout le pipeline - run() spawne automatiquement tous les enfants
    println!("Pipeline started");
    println!("  FileSource: reading from {}", input_path);
    println!("  FlacFileSink: writing to {}", output_path);

    let result = Box::new(source).run(stop_token).await;

    // Vérifier le résultat
    match result {
        Ok(()) => {
            println!();
            println!("✓ Pipeline completed successfully");
            println!("  Output file: {}", output_path);
        }
        Err(e) => {
            eprintln!();
            eprintln!("✗ Pipeline error: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
