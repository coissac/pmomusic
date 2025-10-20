//! Exemple de streaming audio en temps réel
//!
//! Démontre l'utilisation du pipeline avec génération de chunks
//! en temps réel avec timing approprié

use pmoaudio::{SinkNode, SourceNode, TimerNode};

#[tokio::main]
async fn main() {
    println!("=== Streaming Demo ===\n");
    println!("Streaming audio in real-time for 3 seconds...\n");

    let mut source = SourceNode::new();
    let (mut timer, timer_tx) = TimerNode::new(20);
    let (sink, sink_tx) = SinkNode::new("Streaming Output".to_string(), 20);

    source.add_subscriber(timer_tx);
    timer.add_subscriber(sink_tx);

    let timer_handle = timer.get_position_handle();

    // Spawn le pipeline
    tokio::spawn(async move {
        timer.run().await.unwrap();
    });

    let sink_handle = tokio::spawn(async move {
        sink.run_with_logging().await.unwrap();
    });

    // Monitor la position
    let monitor_handle = tokio::spawn(async move {
        for _ in 0..15 {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            let position = timer_handle.position_sec().await;
            println!("Playback position: {:.3} sec", position);
        }
    });

    // Stream des chunks avec timing réel
    // 100ms par chunk à 48kHz = 4800 samples
    source
        .stream_chunks(4800, 48000, 440.0, 3000) // 3 secondes
        .await
        .unwrap();

    println!("\nStreaming complete.");

    // Attendre la fin
    sink_handle.await.unwrap();
    monitor_handle.await.unwrap();
}
