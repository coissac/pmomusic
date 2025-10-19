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
    gain: f32,
}

impl DspNode {
    pub fn new(channel_size: usize, gain: f32) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (tx, rx) = mpsc::channel(channel_size);

        let node = Self {
            rx,
            subscribers: MultiSubscriberNode::new(),
            gain,
        };

        (node, tx)
    }

    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioChunk>>) {
        self.subscribers.add_subscriber(tx);
    }

    /// Applique le gain aux chunks
    pub async fn run(mut self) -> Result<(), AudioError> {
        while let Some(chunk) = self.rx.recv().await {
            if (self.gain - 1.0).abs() < f32::EPSILON {
                // Gain = 1.0, pas de transformation nécessaire
                self.subscribers.push(chunk).await?;
            } else {
                // Clone les données pour les modifier
                let (mut left_data, mut right_data) = chunk.clone_data();

                // Appliquer le gain
                for sample in &mut left_data {
                    *sample *= self.gain;
                }
                for sample in &mut right_data {
                    *sample *= self.gain;
                }

                let new_chunk =
                    AudioChunk::new(chunk.order, left_data, right_data, chunk.sample_rate);

                self.subscribers.push(Arc::new(new_chunk)).await?;
            }
        }
        Ok(())
    }

    /// Met à jour le gain dynamiquement (nécessite un `Arc<RwLock<f32>>` dans une version réelle)
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
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
            let (left_data, right_data) = chunk.clone_data();
            let mut new_left = Vec::with_capacity(left_data.len());
            let mut new_right = Vec::with_capacity(right_data.len());

            // Appliquer le filtre
            for &sample in &left_data {
                self.prev_left = self.prev_left + self.alpha * (sample - self.prev_left);
                new_left.push(self.prev_left);
            }

            for &sample in &right_data {
                self.prev_right = self.prev_right + self.alpha * (sample - self.prev_right);
                new_right.push(self.prev_right);
            }

            let new_chunk = AudioChunk::new(chunk.order, new_left, new_right, chunk.sample_rate);

            self.subscribers.push(Arc::new(new_chunk)).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dsp_node_unity_gain() {
        let (mut node, tx) = DspNode::new(10, 1.0);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        // Envoyer un chunk
        let chunk = AudioChunk::new(0, vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0], 48000);
        let chunk_arc = Arc::new(chunk);
        tx.send(chunk_arc.clone()).await.unwrap();

        // Avec gain = 1.0, le chunk ne devrait pas être cloné
        let received = out_rx.recv().await.unwrap();
        assert!(Arc::ptr_eq(&chunk_arc, &received));
    }

    #[tokio::test]
    async fn test_dsp_node_gain() {
        let (mut node, tx) = DspNode::new(10, 2.0);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        // Envoyer un chunk
        let chunk = AudioChunk::new(0, vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0], 48000);
        tx.send(Arc::new(chunk)).await.unwrap();

        // Vérifier que le gain a été appliqué
        let received = out_rx.recv().await.unwrap();
        assert_eq!(received.left[0], 2.0);
        assert_eq!(received.left[1], 4.0);
        assert_eq!(received.left[2], 6.0);
        assert_eq!(received.right[0], 8.0);
        assert_eq!(received.right[1], 10.0);
        assert_eq!(received.right[2], 12.0);
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
        let chunk = AudioChunk::new(
            0,
            vec![1.0, 1.0, 1.0, -1.0, -1.0, -1.0],
            vec![1.0, 1.0, 1.0, -1.0, -1.0, -1.0],
            48000,
        );
        tx.send(Arc::new(chunk)).await.unwrap();

        // Le filtre devrait lisser le signal
        let received = out_rx.recv().await.unwrap();

        // Vérifier que le signal est lissé (valeurs intermédiaires)
        assert!(received.left[0].abs() < 1.0); // Premier échantillon lissé
        assert!(received.left[2].abs() < 1.0); // Signal ne devrait pas atteindre 1.0 immédiatement
    }

    #[tokio::test]
    async fn test_dsp_node_multiple_subscribers() {
        let (mut node, tx) = DspNode::new(10, 0.5);
        let (out_tx1, mut out_rx1) = mpsc::channel(10);
        let (out_tx2, mut out_rx2) = mpsc::channel(10);

        node.add_subscriber(out_tx1);
        node.add_subscriber(out_tx2);

        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        let chunk = AudioChunk::new(0, vec![2.0, 4.0], vec![2.0, 4.0], 48000);
        tx.send(Arc::new(chunk)).await.unwrap();

        // Les deux abonnés devraient recevoir le même Arc
        let received1 = out_rx1.recv().await.unwrap();
        let received2 = out_rx2.recv().await.unwrap();

        assert!(Arc::ptr_eq(&received1, &received2));
        assert_eq!(received1.left[0], 1.0); // 2.0 * 0.5
        assert_eq!(received1.left[1], 2.0); // 4.0 * 0.5
    }
}
