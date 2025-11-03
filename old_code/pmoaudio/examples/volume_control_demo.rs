//! Exemple simple de contrôle de volume
//!
//! Démontre l'utilisation du VolumeNode avec changements dynamiques

use pmoaudio::{SinkNode, SourceNode, VolumeNode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Volume Control Demo ===\n");

    // Créer la source
    let mut source = SourceNode::new();

    // Créer le volume node
    let (mut volume, volume_tx) = VolumeNode::new("main".to_string(), 1.0, 10);
    let volume_handle = volume.get_handle();

    // Créer le sink
    let (sink, sink_tx) = SinkNode::new("Output".to_string(), 10);

    // Connecter le pipeline
    source.add_subscriber(volume_tx);
    volume.add_subscriber(sink_tx);

    // Lancer les nodes
    tokio::spawn(async move { volume.run().await.unwrap() });

    let sink_handle = tokio::spawn(async move { sink.run_with_stats().await.unwrap() });

    // Contrôler le volume pendant la lecture
    let volume_control = tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        println!("Setting volume to 0.5");
        volume_handle.set_volume(0.5).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        println!("Setting volume to 0.2");
        volume_handle.set_volume(0.2).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        println!("Setting volume to 1.0");
        volume_handle.set_volume(1.0).await;
    });

    // Générer l'audio
    source
        .generate_chunks(20, 4800, 48000, 440.0)
        .await
        .unwrap();

    volume_control.await?;
    let stats = sink_handle.await?;

    println!("\nFinal statistics:");
    stats.display();

    Ok(())
}
