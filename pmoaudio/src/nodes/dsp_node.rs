use crate::{
    nodes::{AudioError, MultiSubscriberNode},
    AudioChunk,
};
use std::sync::Arc;
use tokio::sync::mpsc;

/// DspNode - Applique des transformations DSP aux chunks audio
///
/// Clone les données uniquement si elles doivent être modifiées
pub struct DspNode {
    rx: mpsc::Receiver<Arc<AudioChunk>>,
    subscribers: MultiSubscriberNode,
    gain_db: f32,
}

impl DspNode {
    pub fn new(channel_size: usize, gain_db: f32) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (tx, rx) = mpsc::channel(channel_size);

        let node = Self {
            rx,
            subscribers: MultiSubscriberNode::new(),
            gain_db,
        };

        (node, tx)
    }

    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioChunk>>) {
        self.subscribers.add_subscriber(tx);
    }

    /// Applique le gain aux chunks
    pub async fn run(mut self) -> Result<(), AudioError> {
        while let Some(chunk) = self.rx.recv().await {
            if self.gain_db.abs() < f32::EPSILON {
                // Gain = 0 dB, pas de transformation nécessaire
                self.subscribers.push(chunk).await?;
                continue;
            }

            let gain_linear = AudioChunk::gain_linear_from_db(self.gain_db as f64) as f32;
            let mut pairs = chunk.to_pairs_f32();
            for frame in &mut pairs {
                frame[0] *= gain_linear;
                frame[1] *= gain_linear;
            }

            let mut new_chunk = AudioChunk::from_pairs_f32(
                chunk.order(),
                pairs,
                chunk.sample_rate(),
                chunk.bit_depth(),
            );
            if chunk.gain_db().abs() > f64::EPSILON {
                new_chunk = new_chunk.set_gain_db(chunk.gain_db());
            }
            self.subscribers.push(new_chunk).await?;
        }
        Ok(())
    }

    /// Met à jour le gain dynamiquement (nécessite un `Arc<RwLock<f32>>` dans une version réelle)
    pub fn set_gain_db(&mut self, gain_db: f32) {
        self.gain_db = gain_db;
    }
}

/// DspNode avec filtre passe-bas simple (mock)
#[allow(dead_code)]
pub struct LowPassDspNode {
    rx: mpsc::Receiver<Arc<AudioChunk>>,
    subscribers: MultiSubscriberNode,
    alpha: f32, // Coefficient du filtre
    prev_left: f32,
    prev_right: f32,
}

impl LowPassDspNode {
    #[allow(dead_code)]
    pub fn new(channel_size: usize, cutoff_ratio: f32) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (tx, rx) = mpsc::channel(channel_size);

        // Filtre RC simple: alpha = dt / (RC + dt)
        // cutoff_ratio entre 0 (tout couper) et 1 (tout passer)
        let alpha = cutoff_ratio.clamp(0.0, 1.0);

        let node = Self {
            rx,
            subscribers: MultiSubscriberNode::new(),
            alpha,
            prev_left: 0.0,
            prev_right: 0.0,
        };

        (node, tx)
    }

    #[allow(dead_code)]
    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioChunk>>) {
        self.subscribers.add_subscriber(tx);
    }

    #[allow(dead_code)]
    pub async fn run(mut self) -> Result<(), AudioError> {
        while let Some(chunk) = self.rx.recv().await {
            let pairs = chunk.to_pairs_f32();
            let mut filtered = Vec::with_capacity(pairs.len());

            for sample in pairs.iter() {
                self.prev_left = self.prev_left + self.alpha * (sample[0] - self.prev_left);
                self.prev_right = self.prev_right + self.alpha * (sample[1] - self.prev_right);
                filtered.push([self.prev_left, self.prev_right]);
            }

            let mut new_chunk = AudioChunk::from_pairs_f32(
                chunk.order(),
                filtered,
                chunk.sample_rate(),
                chunk.bit_depth(),
            );
            if chunk.gain_db().abs() > f64::EPSILON {
                new_chunk = new_chunk.set_gain_db(chunk.gain_db());
            }

            self.subscribers.push(new_chunk).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BitDepth;

    #[tokio::test]
    async fn test_dsp_node_unity_gain() {
        let (mut node, tx) = DspNode::new(10, 0.0);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        // Envoyer un chunk
        let chunk = AudioChunk::from_channels_f32(
            0,
            vec![0.25, 0.5, 0.75],
            vec![0.1, 0.2, 0.3],
            48000,
            BitDepth::B24,
        );
        tx.send(chunk.clone()).await.unwrap();

        // Avec gain = 1.0, le chunk ne devrait pas être cloné
        let received = out_rx.recv().await.unwrap();
        assert!(Arc::ptr_eq(&chunk, &received));
    }

    #[tokio::test]
    async fn test_dsp_node_gain() {
        let gain_db = AudioChunk::gain_db_from_linear(2.0) as f32;
        let (mut node, tx) = DspNode::new(10, gain_db);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        // Envoyer un chunk
        let chunk = AudioChunk::from_channels_f32(
            0,
            vec![0.25, 0.5, 0.75],
            vec![0.1, 0.2, 0.3],
            48000,
            BitDepth::B24,
        );
        tx.send(chunk).await.unwrap();

        // Vérifier que le gain a été appliqué
        let received = out_rx.recv().await.unwrap();
        let frames = received.to_pairs_f32();
        const EPS: f32 = 1e-3;
        assert!((frames[0][0] - 0.5).abs() < EPS);
        assert!((frames[1][0] - 1.0).abs() < EPS);
        assert!((frames[2][0] - 1.0).abs() < EPS); // Clamp at full scale
        assert!((frames[0][1] - 0.2).abs() < EPS);
        assert!((frames[1][1] - 0.4).abs() < EPS);
        assert!((frames[2][1] - 0.6).abs() < EPS);
    }

    #[tokio::test]
    async fn test_lowpass_dsp_node() {
        let (mut node, tx) = LowPassDspNode::new(10, 0.5);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        // Envoyer un chunk avec un signal carré
        let chunk = AudioChunk::from_channels_f32(
            0,
            vec![1.0, 1.0, 1.0, -1.0, -1.0, -1.0],
            vec![1.0, 1.0, 1.0, -1.0, -1.0, -1.0],
            48000,
            BitDepth::B24,
        );
        tx.send(chunk).await.unwrap();

        // Le filtre devrait lisser le signal
        let received = out_rx.recv().await.unwrap();
        let frames = received.to_pairs_f32();
        assert!(frames[0][0].abs() < 1.0); // Premier échantillon lissé
        assert!(frames[2][0].abs() < 1.0); // Signal ne devrait pas atteindre 1.0 immédiatement
    }

    #[tokio::test]
    async fn test_dsp_node_multiple_subscribers() {
        let gain_db = AudioChunk::gain_db_from_linear(0.5) as f32;
        let (mut node, tx) = DspNode::new(10, gain_db);
        let (out_tx1, mut out_rx1) = mpsc::channel(10);
        let (out_tx2, mut out_rx2) = mpsc::channel(10);

        node.add_subscriber(out_tx1);
        node.add_subscriber(out_tx2);

        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        let chunk =
            AudioChunk::from_channels_f32(0, vec![0.8, 0.4], vec![0.8, 0.4], 48000, BitDepth::B24);
        tx.send(chunk).await.unwrap();

        // Les deux abonnés devraient recevoir le même Arc
        let received1 = out_rx1.recv().await.unwrap();
        let received2 = out_rx2.recv().await.unwrap();

        assert!(Arc::ptr_eq(&received1, &received2));
        let frames = received1.to_pairs_f32();
        const EPS: f32 = 1e-3;
        assert!((frames[0][0] - 0.4).abs() < EPS); // 0.8 * 0.5
        assert!((frames[1][0] - 0.2).abs() < EPS); // 0.4 * 0.5
    }
}
