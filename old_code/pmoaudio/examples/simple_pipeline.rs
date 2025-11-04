//! Exemple simple de pipeline audio : Source → Timer → Sink
//!
//! Démontre l'utilisation basique du pipeline avec calcul de position

use pmoaudio::{SinkNode, SourceNode, TimerNode};

#[tokio::main]
async fn main() {
    println!("=== Simple Pipeline Example ===\n");

    // Créer les nodes
    let (mut timer, timer_tx) = TimerNode::new(10);
    let (sink, sink_tx) = SinkNode::new("Output".to_string(), 10);

    // Connecter
    timer.add_subscriber(sink_tx);

    // Handle pour monitorer la position
    let timer_handle = timer.get_position_handle();

    // Spawn timer et sink
    tokio::spawn(async move {
        timer.run().await.unwrap();
    });

    let sink_handle = tokio::spawn(async move {
        let stats = sink.run_with_stats().await.unwrap();
        stats.display();
        stats
    });

    // Générer quelques secondes d'audio dans une tâche séparée
    println!("Generating 440Hz sine wave...\n");

    tokio::spawn(async move {
        let mut source = SourceNode::new();
        source.add_subscriber(timer_tx);

        source
            .generate_chunks(30, 4800, 48000, 440.0) // ~3 secondes
            .await
            .unwrap();

        // La source est drop ici, fermant le channel
    });

    // Attendre la fin
    let stats = sink_handle.await.unwrap();

    let final_position = timer_handle.position_sec().await;
    println!("\nFinal position: {:.3} seconds", final_position);
    println!("Total duration: {:.3} seconds", stats.total_duration_sec);
}
