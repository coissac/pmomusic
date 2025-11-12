//! Nodes du pipeline audio
//!
//! Ce module contient tous les types de nodes disponibles pour construire
//! un pipeline audio, ainsi que les traits et structures de support.

use std::sync::Arc;

use crate::type_constraints::{TypeMismatch, TypeRequirement};
use crate::AudioSegment;

/// Taille par défaut du buffer de channel MPSC pour les nodes
/// Cette valeur détermine combien de segments audio peuvent être mis en attente
/// avant que le producteur soit bloqué (backpressure).
pub const DEFAULT_CHANNEL_SIZE: usize = 16;

/// Durée par défaut des chunks audio en millisecondes
/// Cette valeur détermine la latence de traitement et le compromis efficacité/réactivité.
/// 50ms offre un bon équilibre pour la plupart des applications de lecture audio.
pub const DEFAULT_CHUNK_DURATION_MS: f64 = 50.0;

// Modules actifs
pub mod audio_sink;
pub mod converter_nodes;
pub mod file_source;
pub mod flac_file_sink;
pub mod http_source;
pub mod resampling_node;
pub mod timer_node;

// Modules temporairement désactivés
//
// Les modules suivants sont désactivés car ils sont en cours de refactoring
// pour s'aligner avec la nouvelle architecture du pipeline. Ils seront réactivés
// une fois la migration complétée :
//
// - buffer_node : Node de buffer pour stocker des chunks audio
// - chromecast_sink : Sink pour diffuser vers des appareils Chromecast
// - decoder_node : Node de décodage audio générique
// - disk_sink : Sink pour écrire l'audio sur disque (remplacé par flac_file_sink)
// - dsp_node : Node de traitement DSP générique
// - mpd_sink : Sink pour diffuser vers MPD (Music Player Daemon)
// - sink_node : Trait de base pour les sinks (refactorisé dans pipeline.rs)
// - source_node : Trait de base pour les sources (refactorisé dans pipeline.rs)
// - volume_node : Node de contrôle de volume (fonctionnalité intégrée dans AudioChunk)
//
/*
pub mod buffer_node;
pub mod chromecast_sink;
pub mod decoder_node;
pub mod disk_sink;
pub mod dsp_node;
pub mod mpd_sink;
pub mod sink_node;
pub mod source_node;
pub mod volume_node;
*/

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
    async fn push(&mut self, chunk: Arc<AudioSegment>) -> Result<(), AudioError>;

    /// Ferme le node proprement
    async fn close(&mut self);
}

/// Trait pour les nodes qui déclarent leurs types acceptés/produits
///
/// Ce trait permet de vérifier la compatibilité des types entre nodes
/// avant de les connecter dans un pipeline.
///
/// # Exemples
///
/// ```no_run
/// use pmoaudio::{FileSource, FlacFileSink, TypedAudioNode};
/// use pmoaudio::type_constraints::check_compatibility;
///
/// // Vérifier la compatibilité avant de connecter
/// let source = FileSource::new("input.flac");
/// let (sink, tx) = FlacFileSink::new("output.flac");
///
/// let source_output = source.output_type();
/// let sink_input = sink.input_type();
///
/// match check_compatibility(&source_output, &sink_input) {
///     Ok(()) => println!("Types compatibles!"),
///     Err(e) => eprintln!("Types incompatibles: {}", e),
/// }
/// ```
pub trait TypedAudioNode {
    /// Retourne les types que ce node peut accepter en entrée
    ///
    /// Pour les sources (qui ne consomment rien), retourne `None`.
    fn input_type(&self) -> Option<TypeRequirement>;

    /// Retourne les types que ce node peut produire en sortie
    ///
    /// Pour les sinks (qui ne produisent rien), retourne `None`.
    fn output_type(&self) -> Option<TypeRequirement>;

    /// Vérifie si ce node peut accepter les chunks d'un producer donné
    ///
    /// # Erreurs
    ///
    /// Retourne `AudioError::TypeMismatch` si les types sont incompatibles
    fn can_accept_from(&self, producer: &dyn TypedAudioNode) -> Result<(), AudioError> {
        match (producer.output_type(), self.input_type()) {
            (Some(prod), Some(cons)) => crate::type_constraints::check_compatibility(&prod, &cons)
                .map_err(|e| AudioError::TypeMismatch(e)),
            (None, Some(_)) => Err(AudioError::TypeMismatch(TypeMismatch {
                producer: TypeRequirement::any(), // Placeholder
                consumer: self.input_type().unwrap(),
                incompatible_type: None,
            })),
            _ => Ok(()), // Si pas de contrainte, toujours compatible
        }
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
    /// Incompatibilité de types entre nodes
    TypeMismatch(TypeMismatch),
    /// Un nœud enfant s'est terminé prématurément (anormal dans un pipeline descendant)
    ChildFinished,
    /// Un nœud enfant est mort (channel fermé pendant un send)
    ChildDied,
    /// Erreur d'I/O (fichier, réseau, etc.)
    IoError(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::SendError => write!(f, "Failed to send audio chunk"),
            AudioError::ReceiveError => write!(f, "Failed to receive audio chunk"),
            AudioError::ProcessingError(msg) => write!(f, "Processing error: {}", msg),
            AudioError::TypeMismatch(tm) => write!(f, "{}", tm),
            AudioError::ChildFinished => write!(f, "Child node finished prematurely"),
            AudioError::ChildDied => write!(f, "Child node died unexpectedly"),
            AudioError::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for AudioError {}
