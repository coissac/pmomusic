use crate::{
    nodes::{AudioError, MultiSubscriberNode},
    AudioChunk,
};
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
            if chunk.sample_rate() == target_sample_rate {
                // Pas besoin de resampling
                self.subscribers.push(chunk).await?;
            } else {
                // Simuler un resampling (mock simple)
                let ratio = target_sample_rate as f64 / chunk.sample_rate() as f64;
                let new_len = (chunk.len() as f64 * ratio) as usize;

                let pairs = chunk.to_pairs_f32();
                let mut resampled = Vec::with_capacity(new_len);

                // Resampling linéaire simple (mock)
                for i in 0..new_len {
                    let src_pos = i as f64 / ratio;
                    let src_idx = src_pos as usize;

                    if src_idx + 1 < pairs.len() {
                        let frac = src_pos - src_idx as f64;
                        let alpha = (1.0 - frac) as f32;
                        let beta = frac as f32;
                        let left_sample = pairs[src_idx][0] * alpha + pairs[src_idx + 1][0] * beta;
                        let right_sample = pairs[src_idx][1] * alpha + pairs[src_idx + 1][1] * beta;

                        resampled.push([left_sample, right_sample]);
                    } else if src_idx < pairs.len() {
                        resampled.push(pairs[src_idx]);
                    }
                }

                let mut new_chunk = AudioChunk::from_pairs_f32(
                    chunk.order(),
                    resampled,
                    target_sample_rate,
                    chunk.bit_depth(),
                );
                if chunk.gain_db().abs() > f64::EPSILON {
                    new_chunk = new_chunk.set_gain_db(chunk.gain_db());
                }
                self.subscribers.push(new_chunk).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BitDepth;

    #[tokio::test]
    async fn test_decoder_passthrough() {
        let (mut node, tx) = DecoderNode::new(10);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        tokio::spawn(async move {
            node.run_passthrough().await.unwrap();
        });

        // Envoyer un chunk
        let chunk = AudioChunk::from_channels_f32(
            0,
            vec![1.0, 2.0, 3.0],
            vec![4.0, 5.0, 6.0],
            48000,
            BitDepth::B24,
        );
        tx.send(chunk.clone()).await.unwrap();

        // Recevoir le chunk
        let received = out_rx.recv().await.unwrap();
        assert!(Arc::ptr_eq(&chunk, &received));
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
        let chunk =
            AudioChunk::from_channels_f32(0, vec![1.0; 100], vec![1.0; 100], 48000, BitDepth::B24);
        tx.send(chunk).await.unwrap();

        // Recevoir le chunk resampleé
        let received = out_rx.recv().await.unwrap();
        assert_eq!(received.sample_rate(), 96000);
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
        let chunk =
            AudioChunk::from_channels_f32(0, vec![1.0; 100], vec![1.0; 100], 48000, BitDepth::B24);
        tx.send(chunk.clone()).await.unwrap();

        // Le chunk devrait être passé sans modification
        let received = out_rx.recv().await.unwrap();
        assert!(Arc::ptr_eq(&chunk, &received));
    }
}
