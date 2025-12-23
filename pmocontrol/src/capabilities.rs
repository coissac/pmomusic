// pmocontrol/src/capabilities.rs
use anyhow::Result;

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
    fn playback_position(&self) -> Result<PlaybackPositionInfo>;
}

/// High-level playback state across backends.
#[derive(Clone, Debug)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Transitioning,
    NoMedia,
    /// Backend-specific or unknown state string.
    Unknown(String),
}

impl PlaybackState {
    /// Map a raw UPnP AVTransport CurrentTransportState string
    /// to a logical PlaybackState.
    pub fn from_upnp_state(raw: &str) -> Self {
        let s = raw.trim().to_ascii_uppercase();
        match s.as_str() {
            "STOPPED" => PlaybackState::Stopped,
            "PLAYING" => PlaybackState::Playing,
            "PAUSED_PLAYBACK" => PlaybackState::Paused,
            // States from the AVTransport spec that we normalize:
            "PAUSED_RECORDING" => PlaybackState::Paused,
            "RECORDING" => PlaybackState::Playing,
            // Common vendor-specific states:
            "TRANSITIONING" => PlaybackState::Transitioning,
            "BUFFERING" | "PREPARING" => PlaybackState::Transitioning,
            "NO_MEDIA_PRESENT" => PlaybackState::NoMedia,
            _ => PlaybackState::Unknown(raw.to_string()),
        }
    }

    /// Returns a human-readable label for the playback state.
    pub fn as_str(&self) -> &str {
        match self {
            PlaybackState::Stopped => "STOPPED",
            PlaybackState::Playing => "PLAYING",
            PlaybackState::Paused => "PAUSED",
            PlaybackState::Transitioning => "TRANSITIONING",
            PlaybackState::NoMedia => "NO_MEDIA",
            PlaybackState::Unknown(s) => s.as_str(),
        }
    }
}

/// Generic abstraction for playback status (transport state).
///
/// For UPnP AV, this is backed by AVTransport::GetTransportInfo.
/// For OpenHome, a future implementation will adapt from OH Info/Time.
pub trait PlaybackStatus {
    fn playback_state(&self) -> Result<PlaybackState>;
}

/// Abstraction générique des capacités de transport (lecture / pause / stop / seek)
/// indépendamment du protocole sous-jacent (UPnP AV, OpenHome, ...).
pub trait TransportControl {
    /// Set la ressource à lire (URI + métadonnées) et/ou commence la lecture.
    ///
    /// Selon l'implémentation, cette méthode peut soit :
    /// - faire un "Set...URI" + "Play" (cas UPnP AV),
    /// - ou configurer la file de lecture (cas OpenHome, etc.).
    fn play_uri(&self, uri: &str, meta: &str) -> Result<()>;

    /// Démarre ou reprend la lecture.
    fn play(&self) -> Result<()>;

    /// Met la lecture en pause.
    fn pause(&self) -> Result<()>;

    /// Arrête la lecture.
    fn stop(&self) -> Result<()>;

    /// Seek à un temps relatif (HH:MM:SS) si supporté.
    fn seek_rel_time(&self, hhmmss: &str) -> Result<()>;
}

/// Abstraction générique des capacités de contrôle de volume / mute.
pub trait VolumeControl {
    /// Retourne le volume logique courant (échelle dépendante du renderer).
    fn volume(&self) -> Result<u16>;

    /// Définit le volume logique (échelle dépendante du renderer).
    fn set_volume(&self, v: u16) -> Result<()>;

    /// Indique si le renderer est muet (mute activé).
    fn mute(&self) -> Result<bool>;

    /// Active ou désactive le mute.
    fn set_mute(&self, m: bool) -> Result<()>;
}
