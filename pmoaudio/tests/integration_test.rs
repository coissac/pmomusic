//! Tests d'intégration pour le pipeline audio complet
//!
//! NOTE: Ces tests utilisent l'ancienne API (BufferNode, DecoderNode, etc.)
//! qui a été temporairement désactivée. Ils doivent être réécrits pour
//! utiliser la nouvelle architecture de pipeline (FileSource, HttpSource, FlacFileSink, etc.)

// Désactivé temporairement - ancienne API non disponible
/*
use pmoaudio::{AudioChunk, BufferNode, DecoderNode, DspNode, SinkNode, SourceNode, TimerNode};

#[tokio::test]
async fn test_complete_pipeline() {
    // Créer un pipeline complet : Source → Decoder → DSP → Buffer → Timer → Sink

    let (mut decoder, decoder_tx) = DecoderNode::new(10);
    let gain_db = AudioChunk::gain_db_from_linear(0.5) as f32;
    let (mut dsp, dsp_tx) = DspNode::new(10, gain_db); // Gain de 0.5
    let (mut buffer, buffer_tx) = BufferNode::new(50, 10);
    let (mut timer, timer_tx) = TimerNode::new(10);
    let (sink, sink_tx) = SinkNode::new("Integration Test".to_string(), 10);

    // Connecter le pipeline
    decoder.add_subscriber(dsp_tx);
    dsp.add_subscriber(buffer_tx);
    buffer.add_next_subscriber(timer_tx);
    timer.add_subscriber(sink_tx);

    let timer_handle = timer.get_position_handle();

    // Spawn tous les nodes
    tokio::spawn(async move { decoder.run_passthrough().await.unwrap() });
    tokio::spawn(async move { dsp.run().await.unwrap() });
    tokio::spawn(async move { buffer.run().await.unwrap() });
    tokio::spawn(async move { timer.run().await.unwrap() });

    let sink_handle = tokio::spawn(async move { sink.run_with_stats().await.unwrap() });

    // Générer des chunks
    tokio::spawn(async move {
        let mut source = SourceNode::new();
        source.add_subscriber(decoder_tx);
        source
            .generate_chunks(10, 4800, 48000, 440.0)
            .await
            .unwrap();
    });

    // Attendre la fin
    let stats = sink_handle.await.unwrap();

    // Vérifier les résultats
    assert_eq!(stats.chunks_received, 10);
    assert_eq!(stats.total_samples, 48000);

    // Vérifier que le gain a été appliqué (peak devrait être ~0.5)
    assert!(stats.peak_left < 0.51 && stats.peak_left > 0.49);

    // Vérifier la position
    let position = timer_handle.position_sec().await;
    assert!((position - 1.0).abs() < 0.01); // ~1 seconde
}

#[tokio::test]
async fn test_multiroom_buffering() {
    // Tester le BufferNode avec plusieurs abonnés avec offsets

    let (buffer, buffer_tx) = BufferNode::new(50, 20);

    let (sink1, sink1_tx) = SinkNode::new("Room 1".to_string(), 20);
    let (sink2, sink2_tx) = SinkNode::new("Room 2".to_string(), 20);
    let (sink3, sink3_tx) = SinkNode::new("Room 3".to_string(), 20);

    buffer.add_subscriber_with_offset(sink1_tx, 0).await;
    buffer.add_subscriber_with_offset(sink2_tx, 3).await;
    buffer.add_subscriber_with_offset(sink3_tx, 6).await;

    tokio::spawn(async move { buffer.run().await.unwrap() });

    let sink1_handle = tokio::spawn(async move { sink1.run_with_stats().await.unwrap() });
    let sink2_handle = tokio::spawn(async move { sink2.run_with_stats().await.unwrap() });
    let sink3_handle = tokio::spawn(async move { sink3.run_with_stats().await.unwrap() });

    tokio::spawn(async move {
        let mut source = SourceNode::new();
        source.add_subscriber(buffer_tx);
        source
            .generate_chunks(20, 1000, 48000, 440.0)
            .await
            .unwrap();
    });

    let stats1 = sink1_handle.await.unwrap();
    let stats2 = sink2_handle.await.unwrap();
    let stats3 = sink3_handle.await.unwrap();

    // Room 1 devrait avoir tous les chunks
    assert_eq!(stats1.chunks_received, 20);

    // Room 2 devrait avoir 3 chunks de moins
    assert_eq!(stats2.chunks_received, 17);

    // Room 3 devrait avoir 6 chunks de moins
    assert_eq!(stats3.chunks_received, 14);
}

#[tokio::test]
async fn test_timer_accuracy() {
    // Tester la précision du TimerNode

    let (mut timer, timer_tx) = TimerNode::new(10);
    let (sink, sink_tx) = SinkNode::new("Timer Test".to_string(), 10);

    timer.add_subscriber(sink_tx);
    let timer_handle = timer.get_position_handle();

    tokio::spawn(async move { timer.run().await.unwrap() });
    let sink_handle = tokio::spawn(async move { sink.run_silent().await.unwrap() });

    tokio::spawn(async move {
        let mut source = SourceNode::new();
        source.add_subscriber(timer_tx);

        // 48000 samples à 48kHz = 1 seconde
        source
            .generate_chunks(1, 48000, 48000, 440.0)
            .await
            .unwrap();
    });

    sink_handle.await.unwrap();

    let position = timer_handle.position_sec().await;
    let samples = timer_handle.elapsed_samples().await;

    assert_eq!(samples, 48000);
    assert!((position - 1.0).abs() < 0.0001);
}

#[tokio::test]
async fn test_arc_sharing() {
    // Vérifier que les chunks sont bien partagés via Arc sans copie

    let (mut timer, timer_tx) = TimerNode::new(10);
    let (sink1, sink1_tx) = SinkNode::new("Sink1".to_string(), 10);
    let (sink2, sink2_tx) = SinkNode::new("Sink2".to_string(), 10);

    timer.add_subscriber(sink1_tx);
    timer.add_subscriber(sink2_tx);

    tokio::spawn(async move { timer.run().await.unwrap() });

    let sink1_handle = tokio::spawn(async move { sink1.run_with_stats().await.unwrap() });
    let sink2_handle = tokio::spawn(async move { sink2.run_with_stats().await.unwrap() });

    tokio::spawn(async move {
        let mut source = SourceNode::new();
        source.add_subscriber(timer_tx);
        source.generate_silence(5, 1000, 48000).await.unwrap();
    });

    let stats1 = sink1_handle.await.unwrap();
    let stats2 = sink2_handle.await.unwrap();

    // Les deux sinks devraient avoir reçu les mêmes chunks
    assert_eq!(stats1.chunks_received, 5);
    assert_eq!(stats2.chunks_received, 5);
    assert_eq!(stats1.total_samples, stats2.total_samples);
}
*/
