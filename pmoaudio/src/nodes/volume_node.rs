//! Volume nodes - Contrôle du volume audio
//!
//! Ce module fournit des nodes pour ajuster le volume du flux audio,
//! avec support du volume master/secondaire et notification des changements.

use crate::{
    events::{EventPublisher, VolumeChangeEvent},
    nodes::{AudioError, MultiSubscriberNode},
    AudioChunk,
};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// VolumeNode - Applique un gain au flux audio (contrôle software)
///
/// Ce node modifie le champ `gain` de chaque `AudioChunk` qui le traverse.
/// Le gain est multiplié avec le gain existant du chunk, permettant ainsi
/// une chaîne de contrôles de volume.
///
/// # Caractéristiques
///
/// - Thread-safe : le volume peut être modifié pendant l'exécution via `set_volume`
/// - Notification : émet des événements `VolumeChangeEvent` lors des changements
/// - Master/Slave : peut s'abonner à un volume master pour synchronisation
///
/// # Exemples
///
/// ```no_run
/// use pmoaudio::VolumeNode;
///
/// #[tokio::main]
/// async fn main() {
///     let (volume_node, volume_tx) = VolumeNode::new("Room 1".to_string(), 0.8, 10);
///
///     // Modifier le volume pendant l'exécution
///     let handle = volume_node.get_handle();
///     tokio::spawn(async move {
///         tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
///         handle.set_volume(0.5).await;
///     });
///
///     tokio::spawn(async move { volume_node.run().await.unwrap() });
/// }
/// ```
pub struct VolumeNode {
    /// Channel pour recevoir les chunks audio
    rx: mpsc::Receiver<Arc<AudioChunk>>,

    /// Subscribers pour les chunks modifiés
    subscribers: MultiSubscriberNode,

    /// Volume courant (partagé via RwLock pour lecture/écriture thread-safe)
    volume: Arc<RwLock<f32>>,

    /// Publisher pour les événements de changement de volume
    volume_publisher: EventPublisher<VolumeChangeEvent>,

    /// Identifiant unique du node (pour traçabilité)
    node_id: String,

    /// Receiver pour les événements de volume master (optionnel)
    master_volume_rx: Option<mpsc::Receiver<VolumeChangeEvent>>,
}

impl VolumeNode {
    /// Crée un nouveau VolumeNode
    ///
    /// # Arguments
    ///
    /// * `node_id` - Identifiant unique du node
    /// * `initial_volume` - Volume initial (0.0 à 1.0)
    /// * `channel_size` - Taille du buffer du channel
    pub fn new(
        node_id: String,
        initial_volume: f32,
        channel_size: usize,
    ) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (tx, rx) = mpsc::channel(channel_size);

        let node = Self {
            rx,
            subscribers: MultiSubscriberNode::new(),
            volume: Arc::new(RwLock::new(initial_volume)),
            volume_publisher: EventPublisher::new(),
            node_id,
            master_volume_rx: None,
        };

        (node, tx)
    }

    /// Ajoute un subscriber pour recevoir les chunks audio modifiés
    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioChunk>>) {
        self.subscribers.add_subscriber(tx);
    }

    /// Ajoute un subscriber pour les événements de changement de volume
    pub fn subscribe_volume_events(&mut self, tx: mpsc::Sender<VolumeChangeEvent>) {
        self.volume_publisher.subscribe(tx);
    }

    /// Configure ce node pour écouter un volume master
    ///
    /// Le node appliquera à la fois son volume local ET le volume master reçu.
    pub fn set_master_volume_source(&mut self, rx: mpsc::Receiver<VolumeChangeEvent>) {
        self.master_volume_rx = Some(rx);
    }

    /// Retourne un handle pour contrôler le volume depuis un autre contexte
    pub fn get_handle(&self) -> VolumeHandle {
        VolumeHandle {
            volume: self.volume.clone(),
            node_id: self.node_id.clone(),
            publisher: Arc::new(RwLock::new(self.volume_publisher.clone())),
        }
    }

    /// Démarre la boucle de traitement du VolumeNode
    pub async fn run(mut self) -> Result<(), AudioError> {
        let mut master_volume = 1.0f32;

        loop {
            tokio::select! {
                // Recevoir les chunks audio
                chunk_opt = self.rx.recv() => {
                    match chunk_opt {
                        Some(chunk) => {
                            let local_volume = *self.volume.read().await;
                            let total_volume = local_volume * master_volume;

                            // Créer un nouveau chunk avec le gain modifié
                            let modified_chunk = chunk.with_modified_gain(total_volume);

                            // Envoyer aux subscribers
                            self.subscribers.push(Arc::new(modified_chunk)).await?;
                        }
                        None => {
                            // Channel fermé, terminer
                            break;
                        }
                    }
                }

                // Recevoir les mises à jour du volume master (si configuré)
                master_event_opt = async {
                    if let Some(ref mut rx) = self.master_volume_rx {
                        rx.recv().await
                    } else {
                        // Bloquer indéfiniment si pas de master
                        std::future::pending().await
                    }
                } => {
                    if let Some(event) = master_event_opt {
                        master_volume = event.volume;

                        // Optionnel : re-publier l'événement combiné
                        let local_volume = *self.volume.read().await;
                        let combined_event = VolumeChangeEvent {
                            volume: local_volume * master_volume,
                            source_node_id: self.node_id.clone(),
                        };
                        self.volume_publisher.publish(combined_event).await;
                    }
                }
            }
        }

        Ok(())
    }
}

/// Handle pour contrôler un VolumeNode depuis un autre contexte
///
/// Ce handle permet de modifier le volume et de notifier les subscribers
/// sans avoir accès direct au node.
#[derive(Clone)]
pub struct VolumeHandle {
    volume: Arc<RwLock<f32>>,
    node_id: String,
    publisher: Arc<RwLock<EventPublisher<VolumeChangeEvent>>>,
}

impl VolumeHandle {
    /// Modifie le volume
    ///
    /// # Arguments
    ///
    /// * `new_volume` - Nouveau volume (0.0 à 1.0)
    pub async fn set_volume(&self, new_volume: f32) {
        let clamped = new_volume.clamp(0.0, 1.0);
        *self.volume.write().await = clamped;

        // Publier l'événement de changement
        let event = VolumeChangeEvent {
            volume: clamped,
            source_node_id: self.node_id.clone(),
        };

        self.publisher.read().await.publish(event).await;
    }

    /// Obtient le volume courant
    pub async fn get_volume(&self) -> f32 {
        *self.volume.read().await
    }

    /// Augmente le volume de manière relative
    pub async fn adjust_volume(&self, delta: f32) {
        let current = *self.volume.read().await;
        self.set_volume(current + delta).await;
    }
}

/// HardwareVolumeNode - Contrôle matériel du volume
///
/// Ce node simule un contrôle hardware du volume. Dans une implémentation réelle,
/// il communiquerait avec le driver audio pour ajuster le volume matériel.
///
/// Pour cette version, il agit de manière similaire à `VolumeNode` mais pourrait
/// être étendu pour utiliser des APIs système spécifiques.
pub struct HardwareVolumeNode {
    inner: VolumeNode,
}

impl HardwareVolumeNode {
    /// Crée un nouveau HardwareVolumeNode
    pub fn new(
        node_id: String,
        initial_volume: f32,
        channel_size: usize,
    ) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (inner, tx) = VolumeNode::new(node_id, initial_volume, channel_size);

        (Self { inner }, tx)
    }

    /// Ajoute un subscriber
    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioChunk>>) {
        self.inner.add_subscriber(tx);
    }

    /// Obtient un handle pour contrôler le volume
    pub fn get_handle(&self) -> VolumeHandle {
        self.inner.get_handle()
    }

    /// Démarre la boucle de traitement
    pub async fn run(self) -> Result<(), AudioError> {
        // Dans une vraie implémentation, on communiquerait avec le hardware ici
        // Pour l'instant, délègue au VolumeNode standard
        self.inner.run().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_volume_node_basic() {
        let (mut node, tx) = VolumeNode::new("test".to_string(), 0.5, 10);
        let (out_tx, mut out_rx) = mpsc::channel(10);

        node.add_subscriber(out_tx);

        let handle = tokio::spawn(async move { node.run().await });

        // Envoyer un chunk avec gain 1.0
        let chunk = AudioChunk::with_gain(0, vec![1.0; 100], vec![1.0; 100], 48000, 1.0);
        tx.send(Arc::new(chunk)).await.unwrap();

        // Recevoir le chunk modifié
        let modified = out_rx.recv().await.unwrap();
        assert!((modified.gain - 0.5).abs() < f32::EPSILON);

        drop(tx);
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_volume_handle() {
        let (node, tx) = VolumeNode::new("test".to_string(), 1.0, 10);
        let handle = node.get_handle();

        tokio::spawn(async move { node.run().await });

        // Modifier le volume via le handle
        handle.set_volume(0.3).await;

        let volume = handle.get_volume().await;
        assert!((volume - 0.3).abs() < f32::EPSILON);

        drop(tx);
    }

    #[tokio::test]
    async fn test_volume_events() {
        let (mut node, tx) = VolumeNode::new("test".to_string(), 1.0, 10);
        let (event_tx, mut event_rx) = mpsc::channel(10);

        node.subscribe_volume_events(event_tx);
        let handle = node.get_handle();

        tokio::spawn(async move { node.run().await });

        // Changer le volume
        handle.set_volume(0.7).await;

        // Vérifier l'événement
        let event = event_rx.recv().await.unwrap();
        assert!((event.volume - 0.7).abs() < f32::EPSILON);
        assert_eq!(event.source_node_id, "test");

        drop(tx);
    }

    #[tokio::test]
    async fn test_master_slave_volume() {
        // Créer le master
        let (mut master, master_tx) = VolumeNode::new("master".to_string(), 1.0, 10);
        let (master_event_tx, master_event_rx) = mpsc::channel(10);
        master.subscribe_volume_events(master_event_tx);
        let master_handle = master.get_handle();

        // Créer le slave
        let (mut slave, slave_tx) = VolumeNode::new("slave".to_string(), 0.8, 10);
        slave.set_master_volume_source(master_event_rx);
        let (out_tx, mut out_rx) = mpsc::channel(10);
        slave.add_subscriber(out_tx);

        tokio::spawn(async move { master.run().await });
        tokio::spawn(async move { slave.run().await });

        // Envoyer un chunk au slave
        let chunk = AudioChunk::with_gain(0, vec![1.0; 100], vec![1.0; 100], 48000, 1.0);
        slave_tx.send(Arc::new(chunk)).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Modifier le volume master
        master_handle.set_volume(0.5).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Envoyer un autre chunk
        let chunk2 = AudioChunk::with_gain(1, vec![1.0; 100], vec![1.0; 100], 48000, 1.0);
        slave_tx.send(Arc::new(chunk2)).await.unwrap();

        // Le deuxième chunk devrait avoir un gain de 0.8 * 0.5 = 0.4
        let _first = out_rx.recv().await.unwrap(); // gain = 0.8
        let second = out_rx.recv().await.unwrap(); // gain = 0.4

        assert!((second.gain - 0.4).abs() < 0.01);

        drop(master_tx);
        drop(slave_tx);
    }
}
