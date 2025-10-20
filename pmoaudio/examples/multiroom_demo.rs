//! Exemple de configuration multiroom avec BufferNode
//!
//! Démontre l'utilisation du buffer circulaire pour synchroniser
//! plusieurs sorties avec des délais différents

use pmoaudio::{BufferNode, SinkNode, SourceNode};

#[tokio::main]
async fn main() {
    println!("=== Multiroom Demo ===\n");

    // Buffer avec capacité pour gérer les délais
    let (buffer, buffer_tx) = BufferNode::new(50, 10);

    // Créer 3 sorties avec délais différents
    let (sink1, sink1_tx) = SinkNode::new("Room 1 (no delay)".to_string(), 10);
    let (sink2, sink2_tx) = SinkNode::new("Room 2 (5 chunks delay)".to_string(), 10);
    let (sink3, sink3_tx) = SinkNode::new("Room 3 (10 chunks delay)".to_string(), 10);

    buffer.add_subscriber_with_offset(sink1_tx, 0).await;
    buffer.add_subscriber_with_offset(sink2_tx, 5).await;
    buffer.add_subscriber_with_offset(sink3_tx, 10).await;

    // Spawn buffer et sinks
    tokio::spawn(async move {
        buffer.run().await.unwrap();
    });

    let sink1_handle = tokio::spawn(async move {
        let stats = sink1.run_with_stats().await.unwrap();
        stats.display();
        stats
    });

    let sink2_handle = tokio::spawn(async move {
        let stats = sink2.run_with_stats().await.unwrap();
        stats.display();
        stats
    });

    let sink3_handle = tokio::spawn(async move {
        let stats = sink3.run_with_stats().await.unwrap();
        stats.display();
        stats
    });

    // Générer de l'audio dans une tâche séparée
    println!("Generating audio for multiroom playback...\n");
    tokio::spawn(async move {
        let mut source = SourceNode::new();
        source.add_subscriber(buffer_tx);
        source
            .generate_chunks(30, 4800, 48000, 440.0)
            .await
            .unwrap();
    });

    println!("Waiting for all rooms to finish...\n");

    // Attendre toutes les sorties
    let stats1 = sink1_handle.await.unwrap();
    let stats2 = sink2_handle.await.unwrap();
    let stats3 = sink3_handle.await.unwrap();

    println!("\n=== Multiroom Summary ===");
    println!(
        "{}: {} chunks received",
        stats1.name, stats1.chunks_received
    );
    println!(
        "{}: {} chunks received",
        stats2.name, stats2.chunks_received
    );
    println!(
        "{}: {} chunks received",
        stats3.name, stats3.chunks_received
    );

    println!("\nNote: Delayed rooms receive fewer chunks due to the offset");
}
