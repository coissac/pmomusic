use crate::{
    nodes::{AudioError, MultiSubscriberNode},
    AudioChunk,
};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// TimerNode - Node passthrough qui calcule la position temporelle
///
/// Ce node ne modifie pas les données audio, il les passe directement
/// aux abonnés tout en maintenant un compteur de samples pour calculer
/// la position en secondes.
///
/// # Fonctionnement
///
/// Pour chaque chunk reçu:
/// 1. Incrémente `elapsed_samples += chunk.len()`
/// 2. Calcule `position_sec = elapsed_samples / sample_rate`
/// 3. Push le chunk (sans modification) vers les abonnés
///
/// # Utilisation
///
/// Le TimerNode fournit un [`TimerHandle`] qui permet de lire la position
/// depuis d'autres threads/tasks sans bloquer le pipeline.
///
/// # Exemples
///
/// ```no_run
/// use pmoaudio::TimerNode;
///
/// #[tokio::main]
/// async fn main() {
///     let (mut timer, timer_tx) = TimerNode::new(10);
///     let handle = timer.get_position_handle();
///
///     tokio::spawn(async move {
///         timer.run().await.unwrap();
///     });
///
///     // Lire la position depuis un autre thread
///     let position = handle.position_sec().await;
///     println!("Position: {:.2} sec", position);
/// }
/// ```
pub struct TimerNode {
    rx: mpsc::Receiver<Arc<AudioChunk>>,
    subscribers: MultiSubscriberNode,
    elapsed_samples: Arc<RwLock<u64>>,
    current_sample_rate: Arc<RwLock<u32>>,
}

impl TimerNode {
    /// Crée un nouveau TimerNode
    pub fn new(channel_size: usize) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (tx, rx) = mpsc::channel(channel_size);

        let node = Self {
            rx,
            subscribers: MultiSubscriberNode::new(),
            elapsed_samples: Arc::new(RwLock::new(0)),
            current_sample_rate: Arc::new(RwLock::new(48000)), // Default
        };

        (node, tx)
    }

    /// Ajoute un abonné
    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioChunk>>) {
        self.subscribers.add_subscriber(tx);
    }

    /// Retourne la position actuelle en secondes
    pub async fn position_sec(&self) -> f64 {
        let elapsed = *self.elapsed_samples.read().await;
        let sample_rate = *self.current_sample_rate.read().await;
        elapsed as f64 / sample_rate as f64
    }

    /// Retourne le nombre total d'échantillons écoulés
    pub async fn elapsed_samples(&self) -> u64 {
        *self.elapsed_samples.read().await
    }

    /// Reset le compteur
    pub async fn reset(&self) {
        let mut elapsed = self.elapsed_samples.write().await;
        *elapsed = 0;
    }

    /// Démarre la boucle de traitement du TimerNode
    pub async fn run(mut self) -> Result<(), AudioError> {
        while let Some(chunk) = self.rx.recv().await {
            // Mettre à jour le sample rate si nécessaire
            {
                let mut sr = self.current_sample_rate.write().await;
                if *sr != chunk.sample_rate {
                    *sr = chunk.sample_rate;
                }
            }

            // Incrémenter le compteur d'échantillons
            {
                let mut elapsed = self.elapsed_samples.write().await;
                *elapsed += chunk.len() as u64;
            }

            // Push immédiatement le même chunk vers les abonnés (passthrough)
            self.subscribers.push(chunk).await?;
        }

        Ok(())
    }

    /// Version non-bloquante avec try_push
    pub async fn run_nonblocking(mut self) -> Result<(), AudioError> {
        while let Some(chunk) = self.rx.recv().await {
            {
                let mut sr = self.current_sample_rate.write().await;
                if *sr != chunk.sample_rate {
                    *sr = chunk.sample_rate;
                }
            }

            {
                let mut elapsed = self.elapsed_samples.write().await;
                *elapsed += chunk.len() as u64;
            }

            self.subscribers.try_push(chunk).await?;
        }

        Ok(())
    }

    /// Retourne un handle pour lire la position depuis d'autres threads
    pub fn get_position_handle(&self) -> TimerHandle {
        TimerHandle {
            elapsed_samples: self.elapsed_samples.clone(),
            current_sample_rate: self.current_sample_rate.clone(),
        }
    }
}

/// Handle pour lire la position du TimerNode depuis d'autres threads
///
/// Ce handle peut être cloné et utilisé depuis plusieurs threads/tasks
/// pour monitorer la position de lecture sans bloquer le pipeline.
///
/// # Exemples
///
/// ```no_run
/// use pmoaudio::TimerNode;
///
/// #[tokio::main]
/// async fn main() {
///     let (mut timer, _tx) = TimerNode::new(10);
///     let handle = timer.get_position_handle();
///     let handle_clone = handle.clone();
///
///     // Utiliser depuis plusieurs tasks
///     tokio::spawn(async move {
///         loop {
///             let pos = handle_clone.position_sec().await;
///             println!("Position: {:.2}s", pos);
///             tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
///         }
///     });
/// }
/// ```
#[derive(Clone)]
pub struct TimerHandle {
    elapsed_samples: Arc<RwLock<u64>>,
    current_sample_rate: Arc<RwLock<u32>>,
}

impl TimerHandle {
    /// Retourne la position actuelle en secondes
    pub async fn position_sec(&self) -> f64 {
        let elapsed = *self.elapsed_samples.read().await;
        let sample_rate = *self.current_sample_rate.read().await;
        elapsed as f64 / sample_rate as f64
    }

    /// Retourne le nombre total d'échantillons écoulés
    pub async fn elapsed_samples(&self) -> u64 {
        *self.elapsed_samples.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_timer_node_position_calculation() {
        let (mut node, tx) = TimerNode::new(10);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        let handle = node.get_position_handle();

        // Spawn le node
        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        // Envoyer 3 chunks de 1000 samples à 48000 Hz
        for i in 0..3 {
            let chunk = AudioChunk::new(i, vec![0.0; 1000], vec![0.0; 1000], 48000);
            tx.send(Arc::new(chunk)).await.unwrap();
        }

        // Attendre que les chunks soient traités
        for _ in 0..3 {
            out_rx.recv().await.unwrap();
        }

        // Vérifier la position
        let position = handle.position_sec().await;
        let expected = 3000.0 / 48000.0; // 3 chunks * 1000 samples / 48000 Hz
        assert!((position - expected).abs() < 0.0001);

        let elapsed = handle.elapsed_samples().await;
        assert_eq!(elapsed, 3000);
    }

    #[tokio::test]
    async fn test_timer_node_passthrough() {
        let (mut node, tx) = TimerNode::new(10);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        // Envoyer un chunk
        let chunk = AudioChunk::new(42, vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0], 48000);
        let chunk_arc = Arc::new(chunk);
        tx.send(chunk_arc.clone()).await.unwrap();

        // Recevoir le chunk
        let received = out_rx.recv().await.unwrap();

        // Vérifier que c'est le même Arc (pas de clone des données)
        assert!(Arc::ptr_eq(&chunk_arc, &received));
        assert_eq!(received.order, 42);
    }

    #[tokio::test]
    async fn test_timer_node_sample_rate_change() {
        let (mut node, tx) = TimerNode::new(10);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        let handle = node.get_position_handle();

        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        // Chunk à 48000 Hz
        let chunk1 = AudioChunk::new(0, vec![0.0; 48000], vec![0.0; 48000], 48000);
        tx.send(Arc::new(chunk1)).await.unwrap();
        out_rx.recv().await.unwrap();

        // Après 48000 samples à 48000 Hz = 1 seconde
        let pos1 = handle.position_sec().await;
        assert!((pos1 - 1.0).abs() < 0.0001);

        // Chunk à 96000 Hz
        let chunk2 = AudioChunk::new(1, vec![0.0; 96000], vec![0.0; 96000], 96000);
        tx.send(Arc::new(chunk2)).await.unwrap();
        out_rx.recv().await.unwrap();

        // Position calculée avec le nouveau sample rate
        let pos2 = handle.position_sec().await;
        let expected = (48000.0 + 96000.0) / 96000.0;
        assert!((pos2 - expected).abs() < 0.0001);
    }
}
