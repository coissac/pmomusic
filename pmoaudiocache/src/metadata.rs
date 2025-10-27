//! Module de gestion des métadonnées audio
//!
//! Ce module permet d'extraire et gérer les métadonnées des fichiers audio
//! (titre, artiste, album, durée, etc.)

use anyhow::Result;
use lofty::config::ParseOptions;
use lofty::prelude::*;
use lofty::probe::Probe;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[cfg(feature = "pmoserver")]
use utoipa::ToSchema;

/// Métadonnées d'une piste audio
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "pmoserver", derive(ToSchema))]
pub struct AudioMetadata {
    /// Titre de la piste
    #[cfg_attr(feature = "pmoserver", schema(example = "Wish You Were Here"))]
    pub title: Option<String>,

    /// Artiste de la piste
    #[cfg_attr(feature = "pmoserver", schema(example = "Pink Floyd"))]
    pub artist: Option<String>,

    /// Album de la piste
    #[cfg_attr(feature = "pmoserver", schema(example = "Wish You Were Here"))]
    pub album: Option<String>,

    /// Année de sortie
    #[cfg_attr(feature = "pmoserver", schema(example = 1975))]
    pub year: Option<u32>,

    /// Numéro de piste
    #[cfg_attr(feature = "pmoserver", schema(example = 1))]
    pub track_number: Option<u32>,

    /// Nombre total de pistes
    #[cfg_attr(feature = "pmoserver", schema(example = 5))]
    pub track_total: Option<u32>,

    /// Numéro de disque
    #[cfg_attr(feature = "pmoserver", schema(example = 1))]
    pub disc_number: Option<u32>,

    /// Nombre total de disques
    #[cfg_attr(feature = "pmoserver", schema(example = 1))]
    pub disc_total: Option<u32>,

    /// Genre musical
    #[cfg_attr(feature = "pmoserver", schema(example = "Progressive Rock"))]
    pub genre: Option<String>,

    /// Durée en secondes
    #[cfg_attr(feature = "pmoserver", schema(example = 334))]
    pub duration_secs: Option<u64>,

    /// Taux d'échantillonnage (Hz)
    #[cfg_attr(feature = "pmoserver", schema(example = 44100))]
    pub sample_rate: Option<u32>,

    /// Nombre de canaux
    #[cfg_attr(feature = "pmoserver", schema(example = 2))]
    pub channels: Option<u8>,

    /// Bitrate moyen (kbps)
    #[cfg_attr(feature = "pmoserver", schema(example = 1411))]
    pub bitrate: Option<u32>,
}

impl AudioMetadata {
    /// Extrait les métadonnées depuis un fichier audio taggé
    ///
    /// Fonction interne commune pour extraire les métadonnées depuis un TaggedFile
    fn from_tagged_file(tagged_file: lofty::file::TaggedFile) -> Self {
        let properties = tagged_file.properties();
        let tag = tagged_file
            .primary_tag()
            .or_else(|| tagged_file.first_tag());

        let mut metadata = Self {
            title: None,
            artist: None,
            album: None,
            year: None,
            track_number: None,
            track_total: None,
            disc_number: None,
            disc_total: None,
            genre: None,
            duration_secs: Some(properties.duration().as_secs()),
            sample_rate: properties.sample_rate(),
            channels: properties.channels(),
            bitrate: properties.audio_bitrate(),
        };

        if let Some(tag) = tag {
            metadata.title = tag.title().map(|s| s.to_string());
            metadata.artist = tag.artist().map(|s| s.to_string());
            metadata.album = tag.album().map(|s| s.to_string());
            metadata.year = tag.year();
            metadata.track_number = tag.track();
            metadata.track_total = tag.track_total();
            metadata.disc_number = tag.disk();
            metadata.disc_total = tag.disk_total();
            metadata.genre = tag.genre().map(|s| s.to_string());
        }

        metadata
    }

    /// Extrait les métadonnées d'un fichier audio
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin vers le fichier audio
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmoaudiocache::metadata::AudioMetadata;
    /// use std::path::Path;
    ///
    /// let metadata = AudioMetadata::from_file(Path::new("track.flac")).unwrap();
    /// println!("Titre: {:?}", metadata.title);
    /// ```
    pub fn from_file(path: &Path) -> Result<Self> {
        let tagged_file = Probe::open(path)?.options(ParseOptions::new()).read()?;
        Ok(Self::from_tagged_file(tagged_file))
    }

    /// Crée des métadonnées depuis des données brutes audio
    ///
    /// # Arguments
    ///
    /// * `data` - Données audio brutes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let cursor = std::io::Cursor::new(data);
        let tagged_file = Probe::new(cursor)
            .guess_file_type()?
            .options(ParseOptions::new())
            .read()?;
        Ok(Self::from_tagged_file(tagged_file))
    }

    /// Génère une clé de collection basée sur l'artiste et l'album
    ///
    /// Retourne une clé au format "artist:album" si les deux sont disponibles,
    /// sinon retourne None
    pub fn collection_key(&self) -> Option<String> {
        match (&self.artist, &self.album) {
            (Some(artist), Some(album)) => {
                let normalized_artist = artist.to_lowercase().replace(" ", "_");
                let normalized_album = album.to_lowercase().replace(" ", "_");
                Some(format!("{}:{}", normalized_artist, normalized_album))
            }
            _ => None,
        }
    }

    /// Retourne la durée formatée pour DIDL-Lite (H:MM:SS)
    ///
    /// # Exemple
    ///
    /// ```
    /// use pmoaudiocache::AudioMetadata;
    ///
    /// let metadata = AudioMetadata {
    ///     duration_secs: Some(3665),
    ///     ..Default::default()
    /// };
    ///
    /// assert_eq!(metadata.duration_formatted(), Some("1:01:05".to_string()));
    /// ```
    pub fn duration_formatted(&self) -> Option<String> {
        self.duration_secs.map(|d| {
            let hours = d / 3600;
            let minutes = (d % 3600) / 60;
            let seconds = d % 60;
            format!("{}:{:02}:{:02}", hours, minutes, seconds)
        })
    }

    /// Convertit les métadonnées en Resource DIDL-Lite
    ///
    /// # Arguments
    ///
    /// * `url` - URL de la ressource audio
    ///
    /// # Exemple
    ///
    /// ```no_run
    /// use pmoaudiocache::AudioMetadata;
    ///
    /// let metadata = AudioMetadata {
    ///     duration_secs: Some(180),
    ///     sample_rate: Some(44100),
    ///     channels: Some(2),
    ///     ..Default::default()
    /// };
    ///
    /// let resource = metadata.to_didl_resource("http://localhost:8080/audio/tracks/abc123".into());
    /// assert_eq!(resource.url, "http://localhost:8080/audio/tracks/abc123");
    /// ```
    pub fn to_didl_resource(&self, url: String) -> pmodidl::Resource {
        pmodidl::Resource {
            protocol_info: "http-get:*:audio/flac:*".to_string(),
            bits_per_sample: None,
            sample_frequency: self.sample_rate.map(|sr| sr.to_string()),
            nr_audio_channels: self.channels.map(|ch| ch.to_string()),
            duration: self.duration_formatted(),
            url,
        }
    }
}

impl Default for AudioMetadata {
    fn default() -> Self {
        Self {
            title: None,
            artist: None,
            album: None,
            year: None,
            track_number: None,
            track_total: None,
            disc_number: None,
            disc_total: None,
            genre: None,
            duration_secs: None,
            sample_rate: None,
            channels: None,
            bitrate: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collection_key() {
        let metadata = AudioMetadata {
            title: Some("Wish You Were Here".to_string()),
            artist: Some("Pink Floyd".to_string()),
            album: Some("Wish You Were Here".to_string()),
            year: Some(1975),
            track_number: Some(1),
            track_total: Some(5),
            disc_number: Some(1),
            disc_total: Some(1),
            genre: Some("Progressive Rock".to_string()),
            duration_secs: Some(334),
            sample_rate: Some(44100),
            channels: Some(2),
            bitrate: Some(1411),
        };

        assert_eq!(
            metadata.collection_key(),
            Some("pink_floyd:wish_you_were_here".to_string())
        );
    }

    #[test]
    fn test_collection_key_missing_album() {
        let metadata = AudioMetadata {
            title: Some("Test".to_string()),
            artist: Some("Artist".to_string()),
            album: None,
            year: None,
            track_number: None,
            track_total: None,
            disc_number: None,
            disc_total: None,
            genre: None,
            duration_secs: None,
            sample_rate: None,
            channels: None,
            bitrate: None,
        };

        assert_eq!(metadata.collection_key(), None);
    }
}
