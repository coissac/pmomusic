//! Système d'événements et d'abonnements générique pour les nodes
//!
//! Ce module fournit une infrastructure d'abonnement type-safe permettant
//! à chaque node d'émettre et de recevoir différents types d'événements.

use crate::AudioChunk;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Trait de base pour tous les événements de node
///
/// Chaque type d'événement doit implémenter ce trait pour pouvoir
/// être utilisé dans le système d'abonnement.
pub trait NodeEvent: Send + Sync + Clone + 'static {}

/// Événement : données audio disponibles
#[derive(Debug, Clone)]
pub struct AudioDataEvent {
    pub chunk: Arc<AudioChunk>,
}

impl NodeEvent for AudioDataEvent {}

/// Événement : changement de volume
#[derive(Debug, Clone)]
pub struct VolumeChangeEvent {
    pub volume: f32,
    pub source_node_id: String,
}

impl NodeEvent for VolumeChangeEvent {}

/// Événement : mise à jour du nom de la source
#[derive(Debug, Clone)]
pub struct SourceNameUpdateEvent {
    pub source_name: String,
    pub device_name: Option<String>,
}

impl NodeEvent for SourceNameUpdateEvent {}

/// Trait pour les listeners d'événements
///
/// Les nodes qui souhaitent recevoir des événements d'un type particulier
/// doivent implémenter ce trait pour ce type.
#[async_trait::async_trait]
pub trait NodeListener<E: NodeEvent>: Send + Sync {
    /// Appelé lorsqu'un événement est reçu
    async fn on_event(&self, event: E);
}

/// Gestionnaire d'abonnements pour un type d'événement spécifique
///
/// Permet d'enregistrer des listeners et de broadcaster des événements.
#[derive(Clone)]
pub struct EventPublisher<E: NodeEvent> {
    subscribers: Vec<mpsc::Sender<E>>,
}

impl<E: NodeEvent> EventPublisher<E> {
    /// Crée un nouveau publisher vide
    pub fn new() -> Self {
        Self {
            subscribers: Vec::new(),
        }
    }

    /// Ajoute un subscriber via un channel
    pub fn subscribe(&mut self, tx: mpsc::Sender<E>) {
        self.subscribers.push(tx);
    }

    /// Publie un événement à tous les subscribers
    pub async fn publish(&self, event: E) {
        for tx in &self.subscribers {
            // Utiliser try_send pour éviter de bloquer si un subscriber est lent
            let _ = tx.try_send(event.clone());
        }
    }

    /// Publie un événement de manière bloquante (attend que tous les subscribers reçoivent)
    pub async fn publish_blocking(&self, event: E) {
        for tx in &self.subscribers {
            let _ = tx.send(event.clone()).await;
        }
    }

    /// Retourne le nombre de subscribers actifs
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

impl<E: NodeEvent> Default for EventPublisher<E> {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper pour créer un listener basé sur une closure
pub struct ClosureListener<E: NodeEvent, F>
where
    F: Fn(E) + Send + Sync + 'static,
{
    callback: Arc<F>,
    _phantom: std::marker::PhantomData<E>,
}

impl<E: NodeEvent, F> ClosureListener<E, F>
where
    F: Fn(E) + Send + Sync + 'static,
{
    pub fn new(callback: F) -> Self {
        Self {
            callback: Arc::new(callback),
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<E: NodeEvent, F> NodeListener<E> for ClosureListener<E, F>
where
    F: Fn(E) + Send + Sync + 'static,
{
    async fn on_event(&self, event: E) {
        (self.callback)(event);
    }
}

/// Receiver helper pour consommer des événements depuis un channel
pub struct EventReceiver<E: NodeEvent> {
    rx: mpsc::Receiver<E>,
}

impl<E: NodeEvent> EventReceiver<E> {
    /// Crée un nouveau receiver
    pub fn new(rx: mpsc::Receiver<E>) -> Self {
        Self { rx }
    }

    /// Attend le prochain événement
    pub async fn recv(&mut self) -> Option<E> {
        self.rx.recv().await
    }

    /// Tente de recevoir un événement sans bloquer
    pub fn try_recv(&mut self) -> Result<E, mpsc::error::TryRecvError> {
        self.rx.try_recv()
    }
}

/// Macro pour faciliter la création de publishers multiples dans un node
///
/// # Exemple
///
/// ```ignore
/// struct MyNode {
///     audio_publisher: EventPublisher<AudioDataEvent>,
///     volume_publisher: EventPublisher<VolumeChangeEvent>,
/// }
/// ```
#[macro_export]
macro_rules! publishers {
    ($($field:ident: $event_type:ty),* $(,)?) => {
        $(
            pub $field: $crate::events::EventPublisher<$event_type>,
        )*
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_publisher_basic() {
        let mut publisher = EventPublisher::<VolumeChangeEvent>::new();
        let (tx, mut rx) = mpsc::channel(10);

        publisher.subscribe(tx);

        let event = VolumeChangeEvent {
            volume: 0.5,
            source_node_id: "test".to_string(),
        };

        publisher.publish(event.clone()).await;

        let received = rx.recv().await.unwrap();
        assert_eq!(received.volume, 0.5);
        assert_eq!(received.source_node_id, "test");
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let mut publisher = EventPublisher::<VolumeChangeEvent>::new();
        let (tx1, mut rx1) = mpsc::channel(10);
        let (tx2, mut rx2) = mpsc::channel(10);

        publisher.subscribe(tx1);
        publisher.subscribe(tx2);

        let event = VolumeChangeEvent {
            volume: 0.7,
            source_node_id: "test".to_string(),
        };

        publisher.publish(event.clone()).await;

        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();

        assert_eq!(received1.volume, 0.7);
        assert_eq!(received2.volume, 0.7);
    }

    #[tokio::test]
    async fn test_event_receiver() {
        let (tx, rx) = mpsc::channel(10);
        let mut receiver = EventReceiver::new(rx);

        let event = VolumeChangeEvent {
            volume: 0.3,
            source_node_id: "test".to_string(),
        };

        tx.send(event.clone()).await.unwrap();

        let received = receiver.recv().await.unwrap();
        assert_eq!(received.volume, 0.3);
    }
}
