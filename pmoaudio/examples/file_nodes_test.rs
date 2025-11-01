//! Test d'intégration pour FileSource et FlacFileSink
//!
//! Ce programme teste la chaîne complète :
//! 1. Lecture d'un fichier audio avec FileSource
//! 2. Écriture vers FLAC avec FlacFileSink
//!
//! Usage:
//!   cargo run --example file_nodes_test -- <input_file> <output_file>

use pmoaudio::{FileSource, FlacFileSink};
use std::env;

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
    let mut source = FileSource::new(input_path); // Calcul automatique de la taille des chunks (~50ms)
    let (sink, tx) = FlacFileSink::new(output_path); // Utilise le buffer par défaut (16 segments)

    source.add_subscriber(tx);

    // Lancer le sink dans une tâche séparée
    let sink_handle = tokio::spawn(async move {
        println!("FlacFileSink started");
        let result = sink.run().await;
        println!("FlacFileSink finished");
        result
    });

    // Lancer le source
    println!("FileSource started");
    let source_result = source.run().await;
    println!("FileSource finished");

    // Vérifier les résultats
    match source_result {
        Ok(()) => println!("✓ FileSource completed successfully"),
        Err(e) => {
            eprintln!("✗ FileSource error: {}", e);
            return Err(e.into());
        }
    }

    let stats = sink_handle.await??;
    println!("✓ FlacFileSink completed successfully");
    println!();
    println!("Statistics:");
    println!("  Tracks written: {}", stats.tracks.len());
    for (i, track) in stats.tracks.iter().enumerate() {
        println!("  Track {}:", i);
        println!("    Output file:     {:?}", track.path);
        println!("    Chunks received: {}", track.chunks_received);
        println!("    Total samples:   {}", track.total_samples);
        println!(
            "    Duration:        {:.2} seconds",
            track.total_duration_sec
        );
    }

    Ok(())
}
