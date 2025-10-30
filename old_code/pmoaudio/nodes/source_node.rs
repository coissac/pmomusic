use crate::{
    nodes::{AudioError, MultiSubscriberNode},
    AudioChunk, BitDepth,
};
use std::sync::Arc;
use tokio::sync::mpsc;

/// SourceNode - Génère ou lit des chunks audio depuis une source
///
/// Ce node est la source du pipeline. Version mock pour tests.
pub struct SourceNode {
    subscribers: MultiSubscriberNode,
}

const DEFAULT_BIT_DEPTH: BitDepth = BitDepth::B24;

impl SourceNode {
    pub fn new() -> Self {
        Self {
            subscribers: MultiSubscriberNode::new(),
        }
    }

    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioChunk>>) {
        self.subscribers.add_subscriber(tx);
    }

    /// Génère un chunk de test avec une forme d'onde sinusoïdale
    pub fn generate_test_chunk(
        order: u64,
        size: usize,
        sample_rate: u32,
        frequency: f32,
    ) -> Arc<AudioChunk> {
        let mut left = Vec::with_capacity(size);
        let mut right = Vec::with_capacity(size);

        for i in 0..size {
            let t = (order * size as u64 + i as u64) as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin();
            left.push(sample);
            right.push(sample * 0.8); // Légèrement différent pour la stéréo
        }

        AudioChunk::from_channels_f32(order, left, right, sample_rate, DEFAULT_BIT_DEPTH)
    }

    /// Génère et envoie des chunks de test
    pub async fn generate_chunks(
        &self,
        count: u64,
        chunk_size: usize,
        sample_rate: u32,
        frequency: f32,
    ) -> Result<(), AudioError> {
        for i in 0..count {
            let chunk = Self::generate_test_chunk(i, chunk_size, sample_rate, frequency);
            self.subscribers.push(chunk).await?;
        }
        Ok(())
    }

    /// Génère des chunks silencieux
    pub async fn generate_silence(
        &self,
        count: u64,
        chunk_size: usize,
        sample_rate: u32,
    ) -> Result<(), AudioError> {
        for i in 0..count {
            let stereo = vec![[0i32; 2]; chunk_size];
            let chunk = AudioChunk::new(i, stereo, sample_rate, DEFAULT_BIT_DEPTH);
            self.subscribers.push(chunk).await?;
        }
        Ok(())
    }

    /// Version streaming : génère des chunks continuellement avec délai
    pub async fn stream_chunks(
        &self,
        chunk_size: usize,
        sample_rate: u32,
        frequency: f32,
        duration_ms: u64,
    ) -> Result<(), AudioError> {
        let chunk_duration_ms = (chunk_size as f64 / sample_rate as f64 * 1000.0) as u64;
        let mut order = 0u64;

        let start = tokio::time::Instant::now();
        let duration = tokio::time::Duration::from_millis(duration_ms);

        while start.elapsed() < duration {
            let chunk = Self::generate_test_chunk(order, chunk_size, sample_rate, frequency);
            self.subscribers.push(chunk).await?;

            order += 1;

            // Attendre pour simuler le timing réel
            tokio::time::sleep(tokio::time::Duration::from_millis(chunk_duration_ms)).await;
        }

        Ok(())
    }
}

impl Default for SourceNode {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_source_node_generation() {
        let mut source = SourceNode::new();
        let (tx, mut rx) = mpsc::channel(10);

        source.add_subscriber(tx);

        // Générer 3 chunks
        source.generate_chunks(3, 100, 48000, 440.0).await.unwrap();

        // Vérifier la réception
        for i in 0..3 {
            let chunk = rx.recv().await.unwrap();
            assert_eq!(chunk.order(), i);
            assert_eq!(chunk.len(), 100);
            assert_eq!(chunk.sample_rate(), 48000);
        }
    }

    #[test]
    fn test_sine_wave_generation() {
        let chunk = SourceNode::generate_test_chunk(0, 48000, 48000, 440.0);

        // Vérifier qu'on a bien une sinusoïde
        // À 440 Hz avec 48000 samples/s, on devrait avoir 440 cycles
        let pairs = chunk.to_pairs_f32();
        let left: Vec<f32> = pairs.iter().map(|frame| frame[0]).collect();

        // Trouver les passages par zéro
        let mut zero_crossings = 0;
        for i in 1..left.len() {
            if (left[i - 1] < 0.0 && left[i] >= 0.0) || (left[i - 1] >= 0.0 && left[i] < 0.0) {
                zero_crossings += 1;
            }
        }

        // 440 cycles = 880 passages par zéro (approximativement)
        assert!(zero_crossings > 850 && zero_crossings < 910);
    }

    #[tokio::test]
    async fn test_source_node_silence() {
        let mut source = SourceNode::new();
        let (tx, mut rx) = mpsc::channel(10);

        source.add_subscriber(tx);

        source.generate_silence(2, 100, 48000).await.unwrap();

        for _ in 0..2 {
            let chunk = rx.recv().await.unwrap();
            assert!(chunk.frames().iter().all(|frame| frame[0] == 0));
            assert!(chunk.frames().iter().all(|frame| frame[1] == 0));
        }
    }
}
