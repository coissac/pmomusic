//! Exemple de pipeline audio stéréo complet avec tous les nodes
//!
//! Pipeline: SourceNode → DecoderNode → DspNode → BufferNode → TimerNode → SinkNode(s)

use pmoaudio::{BufferNode, DecoderNode, DspNode, SinkNode, SourceNode, TimerNode};

#[tokio::main]
async fn main() {
    println!("=== PMOAudio Pipeline Demo ===\n");

    // Créer le pipeline de nodes

    // 2. DecoderNode - passthrough dans cet exemple
    let (mut decoder, decoder_tx) = DecoderNode::new(10);

    // 3. DspNode - applique un gain de 0.5
    let (mut dsp, dsp_tx) = DspNode::new(10, 0.5);

    // 4. BufferNode - buffer circulaire pour multiroom
    let (mut buffer, buffer_tx) = BufferNode::new(100, 10);

    // 5. TimerNode - calcule la position temporelle
    let (mut timer, timer_tx) = TimerNode::new(10);

    // 6. SinkNodes - deux destinations finales
    let (sink1, sink1_tx) = SinkNode::new("Main Output".to_string(), 10);
    let (sink2, sink2_tx) = SinkNode::new("Secondary Output".to_string(), 10);

    // Ajouter un abonné au BufferNode avec offset (multiroom simulation)
    let (sink3, sink3_tx) = SinkNode::new("Delayed Output".to_string(), 10);
    buffer.add_subscriber_with_offset(sink3_tx, 5).await; // 5 chunks de retard

    // Connecter le pipeline
    decoder.add_subscriber(dsp_tx);
    dsp.add_subscriber(buffer_tx);
    buffer.add_next_subscriber(timer_tx); // BufferNode -> TimerNode
    timer.add_subscriber(sink1_tx);
    timer.add_subscriber(sink2_tx);

    // Obtenir un handle pour lire la position du TimerNode
    let timer_handle = timer.get_position_handle();

    // Spawn tous les nodes
    let decoder_handle = tokio::spawn(async move {
        decoder.run_passthrough().await.unwrap();
    });

    let dsp_handle = tokio::spawn(async move {
        dsp.run().await.unwrap();
    });

    let buffer_handle = tokio::spawn(async move {
        buffer.run().await.unwrap();
    });

    let timer_handle_task = tokio::spawn(async move {
        timer.run().await.unwrap();
    });

    let sink1_handle = tokio::spawn(async move {
        let stats = sink1.run_with_stats().await.unwrap();
        stats.display();
        stats
    });

    let sink2_handle = tokio::spawn(async move {
        sink2.run_silent().await.unwrap();
    });

    let sink3_handle = tokio::spawn(async move {
        let stats = sink3.run_with_stats().await.unwrap();
        stats.display();
        stats
    });

    // Spawn une tâche pour afficher la position périodiquement
    let position_monitor = tokio::spawn(async move {
        for _ in 0..10 {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            let position = timer_handle.position_sec().await;
            let samples = timer_handle.elapsed_samples().await;
            println!("Position: {:.3} sec ({} samples)", position, samples);
        }
    });

    // Générer des chunks audio
    println!("Generating audio chunks...\n");
    let chunk_size = 4800; // 100ms à 48kHz
    let sample_rate = 48000;
    let frequency = 440.0; // La 440Hz

    // Source node dans une tâche séparée
    tokio::spawn(async move {
        let mut source = SourceNode::new();
        source.add_subscriber(decoder_tx);

        // Générer 50 chunks (environ 5 secondes)
        source
            .generate_chunks(50, chunk_size, sample_rate, frequency)
            .await
            .unwrap();

        println!("\nChunks sent. Processing...\n");
    });

    // Attendre que tous les nodes terminent
    decoder_handle.await.unwrap();
    dsp_handle.await.unwrap();
    buffer_handle.await.unwrap();
    timer_handle_task.await.unwrap();

    let stats1 = sink1_handle.await.unwrap();
    sink2_handle.await.unwrap();
    let stats3 = sink3_handle.await.unwrap();
    position_monitor.await.unwrap();

    println!("\n=== Pipeline Demo Complete ===");
    println!(
        "Main output processed: {} chunks, {:.3} sec",
        stats1.chunks_received, stats1.total_duration_sec
    );
    println!(
        "Delayed output processed: {} chunks, {:.3} sec",
        stats3.chunks_received, stats3.total_duration_sec
    );
}
