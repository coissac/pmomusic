//! Exemple complet de pipeline multiroom avec contrôle de volume
//!
//! Ce programme démontre :
//! - Une source audio unique
//! - Deux branches de sortie : Chromecast et DiskSink
//! - Un volume master avec deux VolumeNodes secondaires synchronisés
//! - Système d'événements pour la communication entre nodes

use pmoaudio::{
    ChromecastConfig, ChromecastSink, DiskSink, DiskSinkConfig, SourceNode, VolumeNode,
};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== PMOAudio Multiroom Volume Demo ===\n");

    // Configuration
    let sample_rate = 48000u32;
    let chunk_size = 4800usize; // 100ms à 48kHz
    let num_chunks = 50; // 5 secondes de lecture
    let frequency = 440.0; // La 440 Hz

    // ===== 1. Créer la source audio =====
    println!("1. Creating audio source...");
    let mut source = SourceNode::new();

    // ===== 2. Créer le volume master =====
    println!("2. Creating master volume node...");
    let (mut master_volume, master_tx) = VolumeNode::new("master".to_string(), 1.0, 50);
    let master_handle = master_volume.get_handle();

    // Channel pour les événements du volume master
    let (master_event_tx, master_event_rx_chromecast) = mpsc::channel(10);
    let (_, master_event_rx_disk) = mpsc::channel(10);

    master_volume.subscribe_volume_events(master_event_tx);

    source.add_subscriber(master_tx);

    // ===== 3. Créer les branches de sortie =====

    // Branche 1: Chromecast avec volume secondaire
    println!("3a. Creating Chromecast output branch...");
    let (mut chromecast_volume, chromecast_volume_tx) =
        VolumeNode::new("chromecast_volume".to_string(), 0.8, 50);

    chromecast_volume.set_master_volume_source(master_event_rx_chromecast);

    let chromecast_config = ChromecastConfig {
        device_address: "192.168.1.100".to_string(),
        device_name: "Living Room".to_string(),
        ..Default::default()
    };

    let (chromecast_sink, chromecast_sink_tx) =
        ChromecastSink::new("chromecast1".to_string(), chromecast_config, 50);

    chromecast_volume.add_subscriber(chromecast_sink_tx);
    master_volume.add_subscriber(chromecast_volume_tx);

    // Branche 2: DiskSink avec volume secondaire
    println!("3b. Creating DiskSink output branch...");
    let (mut disk_volume, disk_volume_tx) = VolumeNode::new("disk_volume".to_string(), 0.9, 50);

    disk_volume.set_master_volume_source(master_event_rx_disk);

    let disk_config = DiskSinkConfig {
        output_dir: std::env::temp_dir().join("pmoaudio_demo"),
        filename: Some("multiroom_output.wav".to_string()),
        ..Default::default()
    };

    let (disk_sink, disk_sink_tx) = DiskSink::new("disk1".to_string(), disk_config, 50);

    disk_volume.add_subscriber(disk_sink_tx);
    master_volume.add_subscriber(disk_volume_tx);

    // ===== 4. Lancer tous les nodes =====
    println!("4. Starting pipeline nodes...\n");

    // Spawn master volume
    let master_volume_handle = tokio::spawn(async move {
        master_volume.run().await.unwrap();
    });

    // Spawn chromecast branch
    let chromecast_volume_handle = tokio::spawn(async move {
        chromecast_volume.run().await.unwrap();
    });

    let chromecast_sink_handle = tokio::spawn(async move {
        let stats = chromecast_sink.run().await.unwrap();
        stats.display();
    });

    // Spawn disk branch
    let disk_volume_handle = tokio::spawn(async move {
        disk_volume.run().await.unwrap();
    });

    let disk_sink_handle = tokio::spawn(async move {
        let stats = disk_sink.run().await.unwrap();
        stats.display();
    });

    // ===== 5. Contrôler le volume pendant la lecture =====
    let master_handle_clone = master_handle.clone();
    tokio::spawn(async move {
        // Attendre un peu, puis diminuer le volume
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        println!("\n>>> Decreasing master volume to 0.7");
        master_handle_clone.set_volume(0.7).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        println!(">>> Decreasing master volume to 0.4");
        master_handle_clone.set_volume(0.4).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        println!(">>> Increasing master volume back to 1.0");
        master_handle_clone.set_volume(1.0).await;
    });

    // ===== 6. Générer et envoyer les chunks audio =====
    println!("5. Generating and streaming audio...");
    tokio::spawn(async move {
        source
            .generate_chunks(num_chunks, chunk_size, sample_rate, frequency)
            .await
            .unwrap();
        println!("\n>>> Audio generation complete!");
    });

    // ===== 7. Attendre la fin de tous les nodes =====
    println!("6. Waiting for all nodes to complete...\n");

    // Attendre que les sinks terminent
    chromecast_sink_handle.await?;
    disk_sink_handle.await?;

    // Nettoyer
    master_volume_handle.abort();
    chromecast_volume_handle.abort();
    disk_volume_handle.abort();

    println!("\n=== Demo completed successfully! ===");
    println!("\nSummary:");
    println!(
        "- Generated {} chunks of {} samples each",
        num_chunks, chunk_size
    );
    println!(
        "- Total duration: {:.2} seconds",
        (num_chunks as usize * chunk_size) as f32 / sample_rate as f32
    );
    println!("- Output to Chromecast: Living Room (192.168.1.100)");
    println!(
        "- Output to file: {}",
        std::env::temp_dir()
            .join("pmoaudio_demo")
            .join("multiroom_output.wav")
            .display()
    );
    println!("- Master volume control demonstrated with live changes");
    println!("\nAll streams received synchronized volume updates!");

    Ok(())
}
