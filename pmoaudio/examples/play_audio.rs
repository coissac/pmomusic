//! Exemple simple de lecture audio avec AudioSink
//!
//! Cet exemple montre comment utiliser AudioSink pour jouer un fichier audio
//! sur la sortie audio standard de la machine.
//!
//! Usage:
//!   cargo run --example play_audio -- <fichier.flac>

use pmoaudio::{AudioSink, FileSource};
use std::env;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialiser le logging
    tracing_subscriber::fmt::init();

    // Récupérer le chemin du fichier depuis les arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <fichier.flac>", args[0]);
        eprintln!("\nExemple:");
        eprintln!("  {} music.flac", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];
    println!("Lecture de: {}", file_path);

    // Créer la source audio (lit le fichier FLAC)
    let mut source = FileSource::new(file_path).await?;

    // Créer le sink audio (joue sur la sortie audio)
    let sink = AudioSink::new();

    // Connecter la source au sink
    source.register(Box::new(sink));

    println!("Démarrage de la lecture...");
    println!("Appuyez sur Ctrl+C pour arrêter");

    // Créer un token d'annulation pour pouvoir arrêter proprement
    let stop_token = CancellationToken::new();
    let stop_token_clone = stop_token.clone();

    // Gérer Ctrl+C pour arrêt propre
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
