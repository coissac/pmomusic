//! Structures de données pour représenter les objets Qobuz

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

/// Désérialiseur flexible pour les IDs qui peuvent être des strings ou des integers
pub(crate) fn deserialize_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    use serde_json::Value;

    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(Error::custom("ID must be a string or number")),
    }
}

/// Représente un artiste Qobuz
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Artist {
    /// Identifiant unique de l'artiste
    #[serde(deserialize_with = "deserialize_id")]
    pub id: String,
    /// Nom de l'artiste
    pub name: String,
    /// URL de l'image de l'artiste (optionnelle)
    #[serde(default)]
    pub image: Option<String>,
    /// URL de l'image cachée localement (via pmocovers)
    #[serde(skip)]
    pub image_cached: Option<String>,
}

/// Représente un album Qobuz
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    /// Identifiant unique de l'album
    #[serde(deserialize_with = "deserialize_id")]
    pub id: String,
    /// Titre de l'album
    pub title: String,
    /// Artiste principal de l'album
    pub artist: Artist,
    /// Nombre de pistes
    #[serde(default)]
    pub tracks_count: Option<u32>,
    /// Durée totale en secondes
    #[serde(default)]
    pub duration: Option<u32>,
    /// Date de sortie (format ISO 8601)
    #[serde(default)]
    pub release_date: Option<String>,
    /// URL de l'image de couverture
    #[serde(default)]
    pub image: Option<String>,
    /// URL de l'image cachée localement (via pmocovers)
    #[serde(skip)]
    pub image_cached: Option<String>,
    /// Indique si l'album est disponible pour le streaming
    #[serde(default = "default_true")]
    pub streamable: bool,
    /// Description de l'album
    #[serde(default)]
    pub description: Option<String>,
    /// Taux d'échantillonnage maximum (Hz)
    #[serde(default)]
    pub maximum_sampling_rate: Option<f64>,
    /// Profondeur de bits maximale
    #[serde(default)]
    pub maximum_bit_depth: Option<u32>,
    /// Genre(s) de l'album
    #[serde(default)]
    pub genres: Vec<String>,
    /// Label de l'album
    #[serde(default)]
    pub label: Option<String>,
}

/// Représente une piste (track) Qobuz
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Identifiant unique de la piste
    #[serde(deserialize_with = "deserialize_id")]
    pub id: String,
    /// Titre de la piste
    pub title: String,
    /// Artiste de la piste (peut différer de l'artiste de l'album)
    pub performer: Option<Artist>,
    /// Album contenant la piste
    pub album: Option<Album>,
    /// Durée en secondes
    pub duration: u32,
    /// Numéro de piste
    pub track_number: u32,
    /// Numéro de disque (pour les albums multi-disques)
    pub media_number: u32,
    /// Indique si la piste est disponible pour le streaming
    #[serde(default = "default_true")]
    pub streamable: bool,
    /// Type MIME du fichier audio (déterminé après obtention de l'URL)
    #[serde(skip)]
    pub mime_type: Option<String>,
    /// Fréquence d'échantillonnage (Hz)
    #[serde(skip)]
    pub sample_rate: Option<u32>,
    /// Profondeur de bits
    #[serde(skip)]
    pub bit_depth: Option<u32>,
    /// Nombre de canaux audio
    #[serde(skip)]
    pub channels: Option<u8>,
}

/// Représente une playlist Qobuz
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    /// Identifiant unique de la playlist
    #[serde(deserialize_with = "deserialize_id")]
    pub id: String,
    /// Nom de la playlist
    pub name: String,
    /// Description de la playlist
    #[serde(default)]
    pub description: Option<String>,
    /// Nombre de pistes
    #[serde(default)]
    pub tracks_count: Option<u32>,
    /// Durée totale en secondes
    #[serde(default)]
    pub duration: Option<u32>,
    /// URL de l'image de la playlist
    #[serde(default)]
    pub image: Option<String>,
    /// URL de l'image cachée localement
    #[serde(skip)]
    pub image_cached: Option<String>,
    /// Indique si c'est une playlist publique
    #[serde(default)]
    pub is_public: bool,
    /// Propriétaire de la playlist
    #[serde(default)]
    pub owner: Option<PlaylistOwner>,
}

/// Propriétaire d'une playlist
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistOwner {
    /// Identifiant de l'utilisateur
    pub id: u64,
    /// Nom de l'utilisateur
    pub name: String,
}

/// Représente un genre musical
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Genre {
    /// Identifiant du genre (peut être None pour "All Genres")
    pub id: Option<u32>,
    /// Nom du genre
    pub name: String,
    /// Genres enfants
    #[serde(default)]
    pub children: Vec<Genre>,
}

/// Résultats de recherche
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchResult {
    /// Albums trouvés
    #[serde(default)]
    pub albums: Vec<Album>,
    /// Artistes trouvés
    #[serde(default)]
    pub artists: Vec<Artist>,
    /// Pistes trouvées
    #[serde(default)]
    pub tracks: Vec<Track>,
    /// Playlists trouvées
    #[serde(default)]
    pub playlists: Vec<Playlist>,
}

/// Informations sur un fichier de streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    /// URL de streaming
    pub url: String,
    /// Type MIME
    pub mime_type: String,
    /// Fréquence d'échantillonnage (kHz)
    pub sampling_rate: f64,
    /// Profondeur de bits
    pub bit_depth: u32,
    /// Format ID Qobuz
    pub format_id: u8,
    /// Date d'expiration de l'URL
    #[serde(skip)]
    pub expires_at: DateTime<Utc>,
}

/// Format audio demandé pour le streaming
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum AudioFormat {
    /// MP3 320 kbps
    Mp3_320 = 5,
    /// FLAC 16 bit / 44.1 kHz (CD Quality)
    Flac_Lossless = 6,
    /// FLAC 24 bit / jusqu'à 96 kHz (Hi-Res)
    Flac_HiRes_96 = 7,
    /// FLAC 24 bit / jusqu'à 192 kHz (Hi-Res+)
    Flac_HiRes_192 = 27,
}

impl AudioFormat {
    /// Retourne l'ID du format pour l'API Qobuz
    pub fn id(&self) -> u8 {
        *self as u8
    }

    /// Retourne une description lisible du format
    pub fn description(&self) -> &'static str {
        match self {
            AudioFormat::Mp3_320 => "MP3 320 kbps",
            AudioFormat::Flac_Lossless => "FLAC 16 bit / 44.1 kHz",
            AudioFormat::Flac_HiRes_96 => "FLAC 24 bit / up to 96 kHz",
            AudioFormat::Flac_HiRes_192 => "FLAC 24 bit / up to 192 kHz",
        }
    }

    /// Retourne le type MIME associé
    pub fn mime_type(&self) -> &'static str {
        match self {
            AudioFormat::Mp3_320 => "audio/mpeg",
            _ => "audio/flac",
        }
    }
}

impl Default for AudioFormat {
    fn default() -> Self {
        AudioFormat::Flac_Lossless
    }
}

// Helper functions
fn default_true() -> bool {
    true
}

impl Artist {
    /// Crée un nouvel artiste avec un ID et un nom
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            image: None,
            image_cached: None,
        }
    }
}

impl Album {
    /// Retourne un titre formaté avec les informations audio si disponibles
    pub fn formatted_title(&self) -> String {
        if let (Some(rate), Some(depth)) = (self.maximum_sampling_rate, self.maximum_bit_depth) {
            format!("{} ({:.0}/{} bit)", self.title, rate / 1000.0, depth)
        } else {
            self.title.clone()
        }
    }

    /// Vérifie si l'album est disponible pour le streaming
    pub fn is_available(&self) -> bool {
        self.streamable
    }
}

impl Track {
    /// Retourne l'artiste à afficher (performer ou artiste de l'album)
    pub fn display_artist(&self) -> Option<&Artist> {
        self.performer
            .as_ref()
            .or_else(|| self.album.as_ref().map(|a| &a.artist))
    }

    /// Retourne le nom de l'album si disponible
    pub fn album_name(&self) -> Option<&str> {
        self.album.as_ref().map(|a| a.title.as_str())
    }

    /// Vérifie si la piste est disponible pour le streaming
    pub fn is_available(&self) -> bool {
        self.streamable
    }
}

impl SearchResult {
    /// Crée un résultat de recherche vide
    pub fn new() -> Self {
        Self::default()
    }

    /// Retourne le nombre total de résultats
    pub fn total_count(&self) -> usize {
        self.albums.len() + self.artists.len() + self.tracks.len() + self.playlists.len()
    }

    /// Vérifie si la recherche n'a retourné aucun résultat
    pub fn is_empty(&self) -> bool {
        self.total_count() == 0
    }
}

/// Types d'albums featured disponibles dans le catalogue Qobuz
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeaturedAlbumType {
    /// Nouveautés
    NewReleases,
    /// Nouveautés complètes
    NewReleasesFull,
    /// Discographie idéale
    IdealDiscography,
    /// Qobuzissime
    Qobuzissims,
    /// Choix de l'éditeur
    EditorPicks,
    /// Prix de la presse
    PressAwards,
}

impl FeaturedAlbumType {
    /// Retourne l'identifiant API pour ce type
    pub fn api_id(&self) -> &'static str {
        match self {
            Self::NewReleases => "new-releases",
            Self::NewReleasesFull => "new-releases-full",
            Self::IdealDiscography => "ideal-discography",
            Self::Qobuzissims => "qobuzissims",
            Self::EditorPicks => "editor-picks",
            Self::PressAwards => "press-awards",
        }
    }
}

/// Tags de playlists disponibles dans le catalogue Qobuz
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistTag {
    /// Hi-Res
    HiRes,
    /// Nouvelles
    New,
    /// Thématiques
    Themes,
    /// Choix d'artistes
    ArtistsChoices,
    /// Labels
    Labels,
    /// Humeurs
    Moods,
    /// Artistes
    Artists,
    /// Événements
    Events,
    /// Auditoriums
    Auditoriums,
    /// Populaires
    Popular,
}

impl PlaylistTag {
    /// Retourne l'identifiant API pour ce tag
    pub fn api_id(&self) -> &'static str {
        match self {
            Self::HiRes => "hi-res",
            Self::New => "new",
            Self::Themes => "focus",
            Self::ArtistsChoices => "danslecasque",
            Self::Labels => "label",
            Self::Moods => "mood",
            Self::Artists => "artist",
            Self::Events => "events",
            Self::Auditoriums => "auditoriums",
            Self::Popular => "popular",
        }
    }

    /// Retourne le titre localisé pour ce tag
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::HiRes => "Playlists (Hi-Res)",
            Self::New => "Playlists (New)",
            Self::Themes => "Playlists (Themes)",
            Self::ArtistsChoices => "Playlists (Artist's Choices)",
            Self::Labels => "Playlists (Labels)",
            Self::Moods => "Playlists (Moods)",
            Self::Artists => "Playlists (Artists)",
            Self::Events => "Playlists (Events)",
            Self::Auditoriums => "Playlists (Auditoriums)",
            Self::Popular => "Playlists (Popular)",
        }
    }

    /// Retourne la liste de tous les tags
    pub fn all() -> &'static [PlaylistTag] {
        &[
            Self::HiRes,
            Self::New,
            Self::Themes,
            Self::ArtistsChoices,
            Self::Labels,
            Self::Moods,
            Self::Artists,
            Self::Events,
            Self::Auditoriums,
            Self::Popular,
        ]
    }
}
