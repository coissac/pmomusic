// pmocontrol/src/capabilities.rs
use std::sync::{Arc, Mutex};

use crate::queue::{HasQueue, MusicQueue};
use crate::{errors::ControlPointError, model::PlaybackState, PlaybackItem};

/// Trait for types that track whether they're playing a continuous stream.
pub trait HasContinuousStream {
    fn continuous_stream(&self) -> &Arc<Mutex<bool>>;
}

/// Backend-specific operations for renderers.
///
/// This trait provides access to backend-specific resources like the queue.
pub trait RendererBackend {
    /// Returns a reference to the queue associated with this backend.
    fn queue(&self) -> &Arc<Mutex<MusicQueue>>;
}

/// Queue-aware transport control operations.
///
/// These operations combine queue management with transport control,
/// allowing navigation (next/previous) and track selection from the queue.
#[allow(dead_code)]
pub trait QueueTransportControl: HasQueue + HasContinuousStream {
    /// Play a specific item from the queue (backend-specific implementation).
    fn play_item(&self, item: &PlaybackItem) -> Result<(), ControlPointError>;

    /// Play from the queue at the current index (or initialize to 0 if not set).
    /// This is the default implementation that handles queue navigation.
    fn play_from_queue(&self) -> Result<(), ControlPointError> {
        let mut queue = self.queue().lock().unwrap();

        let current_index = match queue.current_index()? {
            Some(idx) => idx,
            None => {
                if queue.len()? > 0 {
                    queue.set_index(Some(0))?;
                    0
                } else {
                    return Err(ControlPointError::QueueError("Queue is empty".into()));
                }
            }
        };

        let item = queue
            .get_item(current_index)?
            .ok_or_else(|| ControlPointError::QueueError("Current item not found".into()))?;

        drop(queue);

        let is_stream = crate::music_renderer::is_continuous_stream_url(&item.uri);
        *self.continuous_stream().lock().unwrap() = is_stream;

        self.play_item(&item)
    }

    /// Play the next track from the queue.
    fn play_next(&self) -> Result<(), ControlPointError>;

    /// Play the previous track from the queue.
    #[allow(dead_code)]
    fn play_previous(&self) -> Result<(), ControlPointError>;

    /// Play from a specific index in the queue.
    fn play_from_index(&self, index: usize) -> Result<(), ControlPointError>;
}

/// Logical playback position across backends.
///
/// Times peuvent être soit en secondes, soit en "HH:MM:SS" selon ce que
/// tu préfères pour la façade; ici je reste en String pour garder la
/// même granularité que UPnP sans parser.
#[derive(Clone, Debug)]
pub struct PlaybackPositionInfo {
    pub track: Option<u32>,
    pub rel_time: Option<String>,       // position courante
    pub abs_time: Option<String>,       // si pertinent
    pub track_duration: Option<String>, // durée totale
    pub track_metadata: Option<String>, // DIDL-Lite XML from GetPositionInfo
    pub track_uri: Option<String>,      // Current track URI
}
pub trait PlaybackPosition {
    fn playback_position(&self) -> Result<PlaybackPositionInfo, ControlPointError>;
}

/// Generic abstraction for playback status (transport state).
///
/// For UPnP AV, this is backed by AVTransport::GetTransportInfo.
/// For OpenHome, a future implementation will adapt from OH Info/Time.
pub trait PlaybackStatus {
    fn playback_state(&self) -> Result<PlaybackState, ControlPointError>;
}

/// Abstraction générique des capacités de transport (lecture / pause / stop / seek)
/// indépendamment du protocole sous-jacent (UPnP AV, OpenHome, ...).
pub trait TransportControl {
    /// Set la ressource à lire (URI + métadonnées) et/ou commence la lecture.
    ///
    /// Selon l'implémentation, cette méthode peut soit :
    /// - faire un "Set...URI" + "Play" (cas UPnP AV),
    /// - ou configurer la file de lecture (cas OpenHome, etc.).
    fn play_uri(&self, uri: &str, meta: &str) -> Result<(), ControlPointError>;

    /// Démarre ou reprend la lecture.
    fn play(&self) -> Result<(), ControlPointError>;

    /// Met la lecture en pause.
    fn pause(&self) -> Result<(), ControlPointError>;

    /// Arrête la lecture.
    fn stop(&self) -> Result<(), ControlPointError>;

    /// Seek à un temps relatif (HH:MM:SS) si supporté.
    fn seek_rel_time(&self, hhmmss: &str) -> Result<(), ControlPointError>;
}

/// Abstraction générique des capacités de contrôle de volume / mute.
pub trait VolumeControl {
    /// Retourne le volume logique courant (échelle dépendante du renderer).
    fn volume(&self) -> Result<u16, ControlPointError>;

    /// Définit le volume logique (échelle dépendante du renderer).
    fn set_volume(&self, v: u16) -> Result<(), ControlPointError>;

    /// Indique si le renderer est muet (mute activé).
    fn mute(&self) -> Result<bool, ControlPointError>;

    /// Active ou désactive le mute.
    fn set_mute(&self, m: bool) -> Result<(), ControlPointError>;
}
