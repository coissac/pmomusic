use crate::{AudioChunk, nodes::AudioError};
use std::sync::Arc;
use tokio::sync::mpsc;

/// SinkNode - Node terminal qui consomme les chunks audio
///
/// Version mock pour tests et logging
pub struct SinkNode {
    rx: mpsc::Receiver<Arc<AudioChunk>>,
    name: String,
}

impl SinkNode {
    pub fn new(name: String, channel_size: usize) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (tx, rx) = mpsc::channel(channel_size);

        let node = Self { rx, name };

        (node, tx)
    }

    /// Version silencieuse - consomme les chunks sans action
    pub async fn run_silent(mut self) -> Result<(), AudioError> {
        while let Some(_chunk) = self.rx.recv().await {
            // Ne rien faire, juste consommer
        }
        Ok(())
    }

    /// Version avec logging
    pub async fn run_with_logging(mut self) -> Result<(), AudioError> {
        while let Some(chunk) = self.rx.recv().await {
            println!(
                "[{}] Received chunk #{} - {} samples @ {} Hz",
                self.name,
                chunk.order,
                chunk.len(),
                chunk.sample_rate
            );
        }
        Ok(())
    }

    /// Version avec statistiques
    pub async fn run_with_stats(mut self) -> Result<SinkStats, AudioError> {
        let mut stats = SinkStats::new(self.name.clone());

        while let Some(chunk) = self.rx.recv().await {
            stats.process_chunk(&chunk);
        }

        Ok(stats)
    }

    /// Version mock pour écriture dans un fichier (simule l'écriture)
    pub async fn run_mock_file_writer(mut self) -> Result<usize, AudioError> {
        let mut total_samples = 0;

        while let Some(chunk) = self.rx.recv().await {
            total_samples += chunk.len();
            // Simuler l'écriture avec un petit délai
            tokio::time::sleep(tokio::time::Duration::from_micros(10)).await;
        }

        Ok(total_samples)
    }
}

/// Statistiques collectées par un SinkNode
#[derive(Debug, Clone)]
pub struct SinkStats {
    pub name: String,
    pub chunks_received: u64,
    pub total_samples: u64,
    pub total_duration_sec: f64,
    pub peak_left: f32,
    pub peak_right: f32,
    pub rms_left: f64,
    pub rms_right: f64,
}

impl SinkStats {
    pub fn new(name: String) -> Self {
        Self {
            name,
            chunks_received: 0,
            total_samples: 0,
            total_duration_sec: 0.0,
            peak_left: 0.0,
            peak_right: 0.0,
            rms_left: 0.0,
            rms_right: 0.0,
        }
    }

    pub fn process_chunk(&mut self, chunk: &AudioChunk) {
        self.chunks_received += 1;
        self.total_samples += chunk.len() as u64;
        self.total_duration_sec += chunk.len() as f64 / chunk.sample_rate as f64;

        // Calculer les peaks
        for &sample in chunk.left.iter() {
            if sample.abs() > self.peak_left {
                self.peak_left = sample.abs();
            }
        }

        for &sample in chunk.right.iter() {
            if sample.abs() > self.peak_right {
                self.peak_right = sample.abs();
            }
        }

        // Calculer RMS (moyenne des carrés)
        let sum_squares_left: f64 = chunk.left.iter().map(|&x| (x * x) as f64).sum();
        let sum_squares_right: f64 = chunk.right.iter().map(|&x| (x * x) as f64).sum();

        self.rms_left = ((self.rms_left.powi(2) * (self.total_samples - chunk.len() as u64) as f64
            + sum_squares_left)
            / self.total_samples as f64)
            .sqrt();
        self.rms_right = ((self.rms_right.powi(2) * (self.total_samples - chunk.len() as u64) as f64
            + sum_squares_right)
            / self.total_samples as f64)
            .sqrt();
    }

    pub fn display(&self) {
        println!("\n=== Sink Statistics: {} ===", self.name);
        println!("Chunks received: {}", self.chunks_received);
        println!("Total samples: {}", self.total_samples);
        println!("Total duration: {:.3} sec", self.total_duration_sec);
        println!("Peak L/R: {:.3} / {:.3}", self.peak_left, self.peak_right);
        println!("RMS L/R: {:.3} / {:.3}", self.rms_left, self.rms_right);
        println!("========================\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sink_node_silent() {
        let (node, tx) = SinkNode::new("test".to_string(), 10);

        let handle = tokio::spawn(async move { node.run_silent().await });

        // Envoyer quelques chunks
        for i in 0..3 {
            let chunk = AudioChunk::new(i, vec![0.0; 100], vec![0.0; 100], 48000);
            tx.send(Arc::new(chunk)).await.unwrap();
        }

        drop(tx);
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_sink_node_stats() {
        let (node, tx) = SinkNode::new("test".to_string(), 10);

        let handle = tokio::spawn(async move { node.run_with_stats().await });

        // Envoyer des chunks avec signal connu
        for i in 0..3 {
            let chunk = AudioChunk::new(i, vec![1.0; 1000], vec![0.5; 1000], 48000);
            tx.send(Arc::new(chunk)).await.unwrap();
        }

        drop(tx);
        let stats = handle.await.unwrap().unwrap();

        assert_eq!(stats.chunks_received, 3);
        assert_eq!(stats.total_samples, 3000);
        assert_eq!(stats.peak_left, 1.0);
        assert_eq!(stats.peak_right, 0.5);
        assert!((stats.rms_left - 1.0).abs() < 0.001);
        assert!((stats.rms_right - 0.5).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_sink_node_file_writer() {
        let (node, tx) = SinkNode::new("writer".to_string(), 10);

        let handle = tokio::spawn(async move { node.run_mock_file_writer().await });

        // Envoyer des chunks
        for i in 0..5 {
            let chunk = AudioChunk::new(i, vec![0.0; 100], vec![0.0; 100], 48000);
            tx.send(Arc::new(chunk)).await.unwrap();
        }

        drop(tx);
        let total_samples = handle.await.unwrap().unwrap();

        assert_eq!(total_samples, 500);
    }
}
