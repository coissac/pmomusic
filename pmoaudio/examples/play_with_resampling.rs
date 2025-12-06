//! Exemple de lecture audio avec resampling et conversion de format
//!
//! Cet exemple montre comment construire un pipeline audio complet:
//! FileSource → ResamplingNode → ToI24Node → AudioSink
//!
//! Usage:
//!   cargo run --example play_with_resampling -- <fichier.flac> [sample_rate]

use pmoaudio::{AudioSink, FileSource, ResamplingNode, ToI24Node};
use std::env;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialiser le logging
    tracing_subscriber::fmt::init();

    // Récupérer les arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <fichier.flac> [sample_rate]", args[0]);
        eprintln!("\nExemple:");
        eprintln!("  {} music.flac          # Lecture normale", args[0]);
        eprintln!("  {} music.flac 48000    # Resample vers 48kHz", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];
    let target_sample_rate = if args.len() >= 3 {
        args[2].parse::<u32>()?
    } else {
        48000 // Par défaut
    };

    println!("Lecture de: {}", file_path);
    println!("Sample rate cible: {} Hz", target_sample_rate);

    // Créer la source audio
    let mut source = FileSource::new(file_path).await?;

    // Créer le nœud de resampling
    let mut resampler = ResamplingNode::new(target_sample_rate);

    // Créer le nœud de conversion vers I24
    let mut converter = ToI24Node::new();

    // Créer le sink audio avec volume à 80%
    let sink = AudioSink::with_volume(0.8);

    // Construire le pipeline: Source → Resampler → Converter → Sink
    source.register(Box::new(resampler));
    resampler.register(Box::new(converter));
    converter.register(Box::new(sink));

    println!(
        "Pipeline créé: FileSource → Resampling({} Hz) → ToI24 → AudioSink",
        target_sample_rate
    );
    println!("Démarrage de la lecture...");
    println!("Appuyez sur Ctrl+C pour arrêter");

    // Créer un token d'annulation
    let stop_token = CancellationToken::new();
    let stop_token_clone = stop_token.clone();

    // Gérer Ctrl+C
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for Ctrl+C");
        println!("\nArrêt demandé...");
        stop_token_clone.cancel();
    });

    // Lancer le pipeline
    match Box::new(source).run(stop_token).await {
        Ok(()) => println!("\nLecture terminée"),
        Err(e) => eprintln!("\nErreur pendant la lecture: {}", e),
    }

    Ok(())
}
