use crate::{AudioChunk, nodes::{AudioError, MultiSubscriberNode}};
use std::sync::Arc;
use tokio::sync::mpsc;

/// DecoderNode - Décode des chunks audio
///
/// Version mock qui passe simplement les chunks (ou simule un décodage simple)
pub struct DecoderNode {
    rx: mpsc::Receiver<Arc<AudioChunk>>,
    subscribers: MultiSubscriberNode,
}

impl DecoderNode {
    pub fn new(channel_size: usize) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (tx, rx) = mpsc::channel(channel_size);

        let node = Self {
            rx,
            subscribers: MultiSubscriberNode::new(),
        };

        (node, tx)
    }

    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioChunk>>) {
        self.subscribers.add_subscriber(tx);
    }

    /// Mode passthrough - passe les chunks sans modification
    pub async fn run_passthrough(mut self) -> Result<(), AudioError> {
        while let Some(chunk) = self.rx.recv().await {
            self.subscribers.push(chunk).await?;
        }
        Ok(())
    }

    /// Mode mock décodage - simule un changement de sample rate
    pub async fn run_with_resampling(mut self, target_sample_rate: u32) -> Result<(), AudioError> {
        while let Some(chunk) = self.rx.recv().await {
            if chunk.sample_rate == target_sample_rate {
                // Pas besoin de resampling
                self.subscribers.push(chunk).await?;
            } else {
                // Simuler un resampling (mock simple)
                let ratio = target_sample_rate as f64 / chunk.sample_rate as f64;
                let new_len = (chunk.len() as f64 * ratio) as usize;

                let (left_data, right_data) = chunk.clone_data();
                let mut new_left = Vec::with_capacity(new_len);
                let mut new_right = Vec::with_capacity(new_len);

                // Resampling linéaire simple (mock)
                for i in 0..new_len {
                    let src_pos = i as f64 / ratio;
                    let src_idx = src_pos as usize;

                    if src_idx < left_data.len() - 1 {
                        let frac = src_pos - src_idx as f64;
                        let left_sample =
                            left_data[src_idx] * (1.0 - frac as f32) + left_data[src_idx + 1] * frac as f32;
                        let right_sample =
                            right_data[src_idx] * (1.0 - frac as f32) + right_data[src_idx + 1] * frac as f32;

                        new_left.push(left_sample);
                        new_right.push(right_sample);
                    } else if src_idx < left_data.len() {
                        new_left.push(left_data[src_idx]);
                        new_right.push(right_data[src_idx]);
                    }
                }

                let new_chunk = AudioChunk::new(chunk.order, new_left, new_right, target_sample_rate);
                self.subscribers.push(Arc::new(new_chunk)).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_decoder_passthrough() {
        let (mut node, tx) = DecoderNode::new(10);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        tokio::spawn(async move {
            node.run_passthrough().await.unwrap();
        });

        // Envoyer un chunk
        let chunk = AudioChunk::new(0, vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0], 48000);
        let chunk_arc = Arc::new(chunk);
        tx.send(chunk_arc.clone()).await.unwrap();

        // Recevoir le chunk
        let received = out_rx.recv().await.unwrap();
        assert!(Arc::ptr_eq(&chunk_arc, &received));
    }

    #[tokio::test]
    async fn test_decoder_resampling() {
        let (mut node, tx) = DecoderNode::new(10);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        tokio::spawn(async move {
            node.run_with_resampling(96000).await.unwrap();
        });

        // Envoyer un chunk à 48000 Hz
        let chunk = AudioChunk::new(0, vec![1.0; 100], vec![1.0; 100], 48000);
        tx.send(Arc::new(chunk)).await.unwrap();

        // Recevoir le chunk resampleé
        let received = out_rx.recv().await.unwrap();
        assert_eq!(received.sample_rate, 96000);
        // Le chunk devrait être environ 2x plus grand
        assert!(received.len() > 150 && received.len() < 250);
    }

    #[tokio::test]
    async fn test_decoder_no_resampling_needed() {
        let (mut node, tx) = DecoderNode::new(10);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        tokio::spawn(async move {
            node.run_with_resampling(48000).await.unwrap();
        });

        // Envoyer un chunk déjà au bon sample rate
        let chunk = AudioChunk::new(0, vec![1.0; 100], vec![1.0; 100], 48000);
        let chunk_arc = Arc::new(chunk);
        tx.send(chunk_arc.clone()).await.unwrap();

        // Le chunk devrait être passé sans modification
        let received = out_rx.recv().await.unwrap();
        assert!(Arc::ptr_eq(&chunk_arc, &received));
    }
}
