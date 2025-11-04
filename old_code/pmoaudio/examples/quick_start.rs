//! Quick Start - Démonstration rapide des nouvelles fonctionnalités
//!
//! Cet exemple montre l'utilisation des principales nouvelles fonctionnalités :
//! - VolumeNode avec contrôle dynamique
//! - DiskSink pour écriture sur disque
//! - Pipeline simple et efficace

use pmoaudio::{AudioFileFormat, DiskSink, DiskSinkConfig, SourceNode, VolumeNode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== PMOAudio Quick Start ===\n");

    // 1. Créer la source audio (génère un signal de test)
    let mut source = SourceNode::new();

    // 2. Créer un VolumeNode pour contrôler le volume
    let (mut volume, volume_tx) = VolumeNode::new("main".to_string(), 0.8, 10);
    let volume_handle = volume.get_handle();

    // 3. Créer un DiskSink pour écrire sur disque
    let output_dir = std::env::temp_dir().join("pmoaudio_quickstart");
    let config = DiskSinkConfig {
        output_dir: output_dir.clone(),
        filename: Some("quickstart_output.wav".to_string()),
        format: AudioFileFormat::Wav,
        buffer_size: 50,
    };

    let (disk_sink, disk_tx) = DiskSink::new("disk".to_string(), config, 10);

    // 4. Connecter le pipeline : Source → Volume → DiskSink
    source.add_subscriber(volume_tx);
    volume.add_subscriber(disk_tx);

    println!("Pipeline configured:");
    println!("  SourceNode → VolumeNode (vol=0.8) → DiskSink");
    println!("  Output: {}/quickstart_output.wav\n", output_dir.display());

    // 5. Lancer les nodes
    let volume_handle_clone = volume_handle.clone();
    tokio::spawn(async move {
        volume.run().await.unwrap();
    });

    let disk_handle = tokio::spawn(async move {
        let stats = disk_sink.run().await.unwrap();
        println!("\nDiskSink Statistics:");
        stats.display();
        stats
    });

    // 6. Démonstration du contrôle de volume pendant la lecture
    tokio::spawn(async move {
        println!("Generating audio with volume changes...");

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        println!("  → Volume: 0.8 (initial)");

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        volume_handle_clone.set_volume(0.5).await;
        println!("  → Volume: 0.5 (decreased)");

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        volume_handle_clone.set_volume(1.0).await;
        println!("  → Volume: 1.0 (maximum)");

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        volume_handle_clone.set_volume(0.3).await;
        println!("  → Volume: 0.3 (low)");
    });

    // 7. Générer l'audio (10 chunks de 4800 samples à 48kHz = ~1 seconde)
    source
        .generate_chunks(
            10,    // nombre de chunks
            4800,  // samples par chunk (100ms @ 48kHz)
            48000, // sample rate
            440.0, // fréquence (La 440 Hz)
        )
        .await?;

    // 8. Attendre la fin du traitement
    let stats = disk_handle.await?;

    // 9. Résumé
    println!("\n=== Summary ===");
    println!("✓ Audio file generated successfully");
    println!("✓ {} chunks written", stats.chunks_written);
    println!("✓ Duration: {:.2} seconds", stats.total_duration_sec);
    println!("✓ Volume was dynamically adjusted during playback");
    println!("\nYou can play the file with:");
    println!("  ffplay {}/quickstart_output.wav", output_dir.display());

    Ok(())
}
