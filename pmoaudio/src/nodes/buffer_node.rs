use crate::{AudioChunk, nodes::{AudioError, MultiSubscriberNode}};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Subscriber avec son propre offset dans le buffer
struct BufferSubscriber {
    tx: mpsc::Sender<Arc<AudioChunk>>,
    offset: usize, // Position dans le buffer circulaire
}

/// BufferNode avec buffer circulaire pour support multiroom
///
/// Ce node maintient un buffer circulaire de chunks et permet à plusieurs
/// abonnés de lire avec des offsets différents, ce qui est idéal pour des
/// configurations multiroom où différentes pièces peuvent avoir un léger
/// délai de synchronisation.
///
/// # Fonctionnement
///
/// - Le buffer est implémenté avec un `VecDeque` de taille fixe
/// - Chaque abonné peut avoir un offset indépendant (en nombre de chunks)
/// - Utilise `try_send` pour éviter de bloquer si un abonné est saturé
///
/// # Exemples
///
/// ```no_run
/// use pmoaudio::{BufferNode, SinkNode};
///
/// #[tokio::main]
/// async fn main() {
///     let (buffer, buffer_tx) = BufferNode::new(50, 10);
///
///     let (sink1, sink1_tx) = SinkNode::new("Room 1".to_string(), 10);
///     let (sink2, sink2_tx) = SinkNode::new("Room 2".to_string(), 10);
///
///     // Room 1 sans délai
///     buffer.add_subscriber_with_offset(sink1_tx, 0).await;
///
///     // Room 2 avec 5 chunks de retard
///     buffer.add_subscriber_with_offset(sink2_tx, 5).await;
///
///     tokio::spawn(async move { buffer.run().await.unwrap() });
///     // ... spawn sinks et source
/// }
/// ```
pub struct BufferNode {
    buffer: Arc<RwLock<VecDeque<Arc<AudioChunk>>>>,
    subscribers: Arc<RwLock<Vec<BufferSubscriber>>>,
    buffer_size: usize,
    rx: mpsc::Receiver<Arc<AudioChunk>>,
    next_subscribers: MultiSubscriberNode, // Pour passer au node suivant
}

impl BufferNode {
    /// Crée un nouveau BufferNode
    ///
    /// # Arguments
    /// * `buffer_size` - Taille maximale du buffer circulaire
    /// * `channel_size` - Taille du channel bounded pour backpressure
    pub fn new(buffer_size: usize, channel_size: usize) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (tx, rx) = mpsc::channel(channel_size);

        let node = Self {
            buffer: Arc::new(RwLock::new(VecDeque::with_capacity(buffer_size))),
            subscribers: Arc::new(RwLock::new(Vec::new())),
            buffer_size,
            rx,
            next_subscribers: MultiSubscriberNode::new(),
        };

        (node, tx)
    }

    /// Ajoute un abonné avec un offset spécifique (pour multiroom)
    pub async fn add_subscriber_with_offset(
        &self,
        tx: mpsc::Sender<Arc<AudioChunk>>,
        offset: usize,
    ) {
        let mut subs = self.subscribers.write().await;
        subs.push(BufferSubscriber { tx, offset });
    }

    /// Ajoute un abonné sans offset (commence au chunk courant)
    pub async fn add_subscriber(&self, tx: mpsc::Sender<Arc<AudioChunk>>) {
        self.add_subscriber_with_offset(tx, 0).await;
    }

    /// Ajoute un abonné pour le node suivant (sans buffer)
    pub fn add_next_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioChunk>>) {
        self.next_subscribers.add_subscriber(tx);
    }

    /// Démarre la boucle de traitement du BufferNode
    pub async fn run(mut self) -> Result<(), AudioError> {
        let mut chunk_index = 0usize;

        while let Some(chunk) = self.rx.recv().await {
            // Ajouter au buffer circulaire
            {
                let mut buffer = self.buffer.write().await;
                if buffer.len() >= self.buffer_size {
                    buffer.pop_front();
                }
                buffer.push_back(chunk.clone());
            }

            // Envoyer aux abonnés avec offset
            {
                let buffer = self.buffer.read().await;
                let mut subs = self.subscribers.write().await;

                for sub in subs.iter_mut() {
                    // Calculer l'index dans le buffer en fonction de l'offset
                    let target_index = if chunk_index >= sub.offset {
                        chunk_index - sub.offset
                    } else {
                        continue; // Pas encore assez de données
                    };

                    // Vérifier si le chunk est disponible dans le buffer
                    let buffer_age = chunk_index - target_index;
                    if buffer_age < buffer.len() {
                        let chunk_to_send = &buffer[buffer.len() - buffer_age - 1];
                        // try_send non-bloquant pour éviter de bloquer la source
                        let _ = sub.tx.try_send(chunk_to_send.clone());
                    }
                }
            }

            // Push vers les nodes suivants sans buffer
            self.next_subscribers.try_push(chunk).await?;

            chunk_index += 1;
        }

        Ok(())
    }

    /// Version avec push synchrone au lieu de try_push
    pub async fn run_blocking(mut self) -> Result<(), AudioError> {
        let mut chunk_index = 0usize;

        while let Some(chunk) = self.rx.recv().await {
            // Ajouter au buffer circulaire
            {
                let mut buffer = self.buffer.write().await;
                if buffer.len() >= self.buffer_size {
                    buffer.pop_front();
                }
                buffer.push_back(chunk.clone());
            }

            // Envoyer aux abonnés avec offset
            {
                let buffer = self.buffer.read().await;
                let subs = self.subscribers.read().await;

                for sub in subs.iter() {
                    let target_index = if chunk_index >= sub.offset {
                        chunk_index - sub.offset
                    } else {
                        continue;
                    };

                    let buffer_age = chunk_index - target_index;
                    if buffer_age < buffer.len() {
                        let chunk_to_send = &buffer[buffer.len() - buffer_age - 1];
                        let _ = sub.tx.send(chunk_to_send.clone()).await;
                    }
                }
            }

            // Push vers les nodes suivants
            for _ in 0..self.next_subscribers.subscribers.len() {
                self.next_subscribers.push(chunk.clone()).await?;
            }

            chunk_index += 1;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_buffer_node_basic() {
        let (mut node, tx) = BufferNode::new(10, 5);
        let (out_tx, mut out_rx) = mpsc::channel(5);

        node.add_next_subscriber(out_tx);

        // Spawn le node
        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        // Envoyer des chunks
        for i in 0..3 {
            let chunk = AudioChunk::new(i, vec![0.0; 100], vec![0.0; 100], 48000);
            tx.send(Arc::new(chunk)).await.unwrap();
        }

        // Recevoir les chunks
        for i in 0..3 {
            let chunk = out_rx.recv().await.unwrap();
            assert_eq!(chunk.order, i);
        }
    }

    #[tokio::test]
    async fn test_buffer_node_with_offset() {
        let (node, tx) = BufferNode::new(10, 10);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        // Ajouter un abonné avec offset de 2 chunks
        node.add_subscriber_with_offset(out_tx, 2).await;

        // Spawn le node
        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        // Envoyer 5 chunks
        for i in 0..5 {
            let chunk = AudioChunk::new(i, vec![0.0; 100], vec![0.0; 100], 48000);
            tx.send(Arc::new(chunk)).await.unwrap();
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // L'abonné devrait recevoir les chunks 0, 1, 2 (avec 2 chunks de retard)
        let chunk = out_rx.try_recv().unwrap();
        assert_eq!(chunk.order, 0);
    }
}
