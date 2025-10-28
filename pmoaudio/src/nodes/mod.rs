//! Nodes du pipeline audio
//!
//! Ce module contient tous les types de nodes disponibles pour construire
//! un pipeline audio, ainsi que les traits et structures de support.

use crate::AudioChunk;
use std::sync::Arc;
use tokio::sync::mpsc;

pub mod buffer_node;
pub mod chromecast_sink;
pub mod decoder_node;
pub mod disk_sink;
pub mod dsp_node;
pub mod file_source;
pub mod flac_file_sink;
pub mod mpd_sink;
pub mod sink_node;
pub mod source_node;
pub mod timer_node;
pub mod volume_node;

/// Trait de base pour tous les nodes audio
///
/// Tous les nodes du pipeline implémentent ce trait pour permettre
/// une interface uniforme de traitement des chunks audio.
#[async_trait::async_trait]
pub trait AudioNode: Send + Sync {
    /// Push un chunk vers ce node
    ///
    /// # Erreurs
    ///
    /// Retourne `AudioError::SendError` si l'envoi échoue
    async fn push(&mut self, chunk: Arc<AudioChunk>) -> Result<(), AudioError>;

    /// Ferme le node proprement
    async fn close(&mut self);
}

/// Node avec un seul abonné (pas de clone inutile)
///
/// Optimisé pour les cas où un node n'a qu'un seul destinataire.
/// Le Arc du chunk est simplement transféré sans clonage supplémentaire.
///
/// # Exemples
///
/// ```
/// use pmoaudio::SingleSubscriberNode;
/// use tokio::sync::mpsc;
///
/// let (tx, rx) = mpsc::channel(10);
/// let node = SingleSubscriberNode::new(tx);
/// ```
pub struct SingleSubscriberNode {
    tx: mpsc::Sender<Arc<AudioChunk>>,
}

impl SingleSubscriberNode {
    pub fn new(tx: mpsc::Sender<Arc<AudioChunk>>) -> Self {
        Self { tx }
    }

    pub async fn push(&self, chunk: Arc<AudioChunk>) -> Result<(), AudioError> {
        self.tx.send(chunk).await.map_err(|_| AudioError::SendError)
    }
}

/// Node avec plusieurs abonnés (partage le même Arc)
///
/// Permet de broadcaster un chunk à plusieurs destinations.
/// Tous les abonnés reçoivent le même `Arc<AudioChunk>`, donc pas de copie
/// des données audio - seul le compteur de référence Arc est incrémenté.
///
/// # Exemples
///
/// ```
/// use pmoaudio::MultiSubscriberNode;
/// use tokio::sync::mpsc;
///
/// let mut node = MultiSubscriberNode::new();
/// let (tx1, rx1) = mpsc::channel(10);
/// let (tx2, rx2) = mpsc::channel(10);
///
/// node.add_subscriber(tx1);
/// node.add_subscriber(tx2);
/// // Les deux abonnés recevront les mêmes chunks
/// ```
pub struct MultiSubscriberNode {
    subscribers: Vec<mpsc::Sender<Arc<AudioChunk>>>,
}

impl MultiSubscriberNode {
    pub fn new() -> Self {
        Self {
            subscribers: Vec::new(),
        }
    }

    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioChunk>>) {
        self.subscribers.push(tx);
    }

    pub async fn push(&self, chunk: Arc<AudioChunk>) -> Result<(), AudioError> {
        for tx in &self.subscribers {
            // On partage le même Arc avec tous les abonnés
            tx.send(chunk.clone())
                .await
                .map_err(|_| AudioError::SendError)?;
        }
        Ok(())
    }

    pub async fn try_push(&self, chunk: Arc<AudioChunk>) -> Result<(), AudioError> {
        for tx in &self.subscribers {
            // try_send non-bloquant, ignore si saturé
            let _ = tx.try_send(chunk.clone());
        }
        Ok(())
    }
}

impl Default for MultiSubscriberNode {
    fn default() -> Self {
        Self::new()
    }
}

/// Erreurs possibles dans le pipeline audio
#[derive(Debug, Clone)]
pub enum AudioError {
    /// Échec d'envoi d'un chunk à travers un channel
    SendError,
    /// Échec de réception d'un chunk depuis un channel
    ReceiveError,
    /// Erreur de traitement avec message descriptif
    ProcessingError(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::SendError => write!(f, "Failed to send audio chunk"),
            AudioError::ReceiveError => write!(f, "Failed to receive audio chunk"),
            AudioError::ProcessingError(msg) => write!(f, "Processing error: {}", msg),
        }
    }
}

impl std::error::Error for AudioError {}
