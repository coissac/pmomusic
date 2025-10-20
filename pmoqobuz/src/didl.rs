//! Export des objets Qobuz en format DIDL-Lite
//!
//! Ce module permet de convertir les structures Qobuz (Album, Track, etc.)
//! en objets DIDL-Lite compatibles avec UPnP/DLNA.

use crate::error::{QobuzError, Result};
use crate::models::{Album, Playlist, Track};
use pmodidl::{Container, Item, Resource};

/// Trait pour convertir un objet Qobuz en DIDL-Lite
pub trait ToDIDL {
    /// Convertit l'objet en Container DIDL
    fn to_didl_container(&self, parent_id: &str) -> Result<Container>;

    /// Convertit l'objet en Item DIDL
    fn to_didl_item(&self, parent_id: &str) -> Result<Item>;
}

impl ToDIDL for Album {
    /// Convertit un album en Container DIDL
    ///
    /// # Arguments
    ///
    /// * `parent_id` - ID du container parent
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// let album = client.get_album("12345").await?;
    /// let container = album.to_didl_container("0$qobuz$albums")?;
    /// ```
    fn to_didl_container(&self, parent_id: &str) -> Result<Container> {
        let id = format!("0$qobuz$album${}", self.id);

        Ok(Container {
            id,
            parent_id: parent_id.to_string(),
            restricted: Some("1".to_string()),
            child_count: self.tracks_count.map(|c| c.to_string()),
            title: self.formatted_title(),
            class: "object.container.album.musicAlbum".to_string(),
            containers: Vec::new(),
            items: Vec::new(),
        })
    }

    /// Un album ne peut pas être converti directement en Item
    fn to_didl_item(&self, _parent_id: &str) -> Result<Item> {
        Err(QobuzError::DidlExport(
            "Album cannot be converted to Item, use to_didl_container instead".to_string(),
        ))
    }
}

impl ToDIDL for Track {
    /// Une track ne peut pas être convertie en Container
    fn to_didl_container(&self, _parent_id: &str) -> Result<Container> {
        Err(QobuzError::DidlExport(
            "Track cannot be converted to Container, use to_didl_item instead".to_string(),
        ))
    }

    /// Convertit une track en Item DIDL
    ///
    /// # Arguments
    ///
    /// * `parent_id` - ID du container parent
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// let track = client.get_track("98765").await?;
    /// let item = track.to_didl_item("0$qobuz$album$12345")?;
    /// ```
    fn to_didl_item(&self, parent_id: &str) -> Result<Item> {
        let id = format!("0$qobuz$track${}", self.id);

        // Déterminer l'artiste à afficher
        let artist_name = self
            .display_artist()
            .map(|a| a.name.clone())
            .or_else(|| self.album.as_ref().map(|a| a.artist.name.clone()));

        // Déterminer l'album
        let album_name = self.album_name().map(|s| s.to_string());

        // Déterminer l'image de couverture
        let album_art = self
            .album
            .as_ref()
            .and_then(|a| a.image_cached.clone().or_else(|| a.image.clone()));

        // Créer la ressource (URL de streaming)
        // Note: L'URL sera remplie plus tard via get_stream_url
        let resource = Resource {
            protocol_info: format!(
                "http-get:*:{}:*",
                self.mime_type.as_deref().unwrap_or("audio/flac")
            ),
            bits_per_sample: self.bit_depth.map(|b| b.to_string()),
            sample_frequency: self.sample_rate.map(|r| r.to_string()),
            nr_audio_channels: self.channels.map(|c| c.to_string()),
            duration: Some(format_duration(self.duration)),
            url: format!("qobuz://track/{}", self.id), // URL symbolique
        };

        Ok(Item {
            id,
            parent_id: parent_id.to_string(),
            restricted: Some("1".to_string()),
            title: self.title.clone(),
            creator: artist_name.clone(),
            class: "object.item.audioItem.musicTrack".to_string(),
            artist: artist_name,
            album: album_name,
            genre: None, // Qobuz ne fournit pas le genre au niveau track
            album_art,
            album_art_pk: None,
            date: self.album.as_ref().and_then(|a| a.release_date.clone()),
            original_track_number: Some(self.track_number.to_string()),
            resources: vec![resource],
            descriptions: Vec::new(),
        })
    }
}

impl ToDIDL for Playlist {
    /// Convertit une playlist en Container DIDL
    fn to_didl_container(&self, parent_id: &str) -> Result<Container> {
        let id = format!("0$qobuz$playlist${}", self.id);

        Ok(Container {
            id,
            parent_id: parent_id.to_string(),
            restricted: Some("1".to_string()),
            child_count: self.tracks_count.map(|c| c.to_string()),
            title: self.name.clone(),
            class: "object.container.playlistContainer".to_string(),
            containers: Vec::new(),
            items: Vec::new(),
        })
    }

    /// Une playlist ne peut pas être convertie en Item
    fn to_didl_item(&self, _parent_id: &str) -> Result<Item> {
        Err(QobuzError::DidlExport(
            "Playlist cannot be converted to Item, use to_didl_container instead".to_string(),
        ))
    }
}

/// Formate une durée en secondes au format HH:MM:SS
fn format_duration(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}

/// Convertit une liste de tracks en items DIDL
pub fn tracks_to_didl_items(tracks: &[Track], parent_id: &str) -> Result<Vec<Item>> {
    tracks
        .iter()
        .map(|track| track.to_didl_item(parent_id))
        .collect()
}

/// Convertit une liste d'albums en containers DIDL
pub fn albums_to_didl_containers(albums: &[Album], parent_id: &str) -> Result<Vec<Container>> {
    albums
        .iter()
        .map(|album| album.to_didl_container(parent_id))
        .collect()
}

/// Convertit une liste de playlists en containers DIDL
pub fn playlists_to_didl_containers(
    playlists: &[Playlist],
    parent_id: &str,
) -> Result<Vec<Container>> {
    playlists
        .iter()
        .map(|playlist| playlist.to_didl_container(parent_id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Album, Artist, Track};

    #[test]
    fn test_album_to_didl_container() {
        let album = Album {
            id: "123".to_string(),
            title: "Test Album".to_string(),
            artist: Artist::new("456", "Test Artist"),
            tracks_count: Some(10),
            duration: Some(3000),
            release_date: Some("2024-01-01".to_string()),
            image: None,
            image_cached: None,
            streamable: true,
            description: None,
            maximum_sampling_rate: Some(96000.0),
            maximum_bit_depth: Some(24),
            genres: vec![],
            label: None,
        };

        let container = album.to_didl_container("parent").unwrap();
        assert_eq!(container.id, "0$qobuz$album$123");
        assert_eq!(container.parent_id, "parent");
        assert!(container.title.contains("Test Album"));
    }

    #[test]
    fn test_track_to_didl_item() {
        let track = Track {
            id: "789".to_string(),
            title: "Test Track".to_string(),
            performer: Some(Artist::new("456", "Test Artist")),
            album: None,
            duration: 180,
            track_number: 1,
            media_number: 1,
            streamable: true,
            mime_type: Some("audio/flac".to_string()),
            sample_rate: Some(44100),
            bit_depth: Some(16),
            channels: Some(2),
        };

        let item = track.to_didl_item("parent").unwrap();
        assert_eq!(item.id, "0$qobuz$track$789");
        assert_eq!(item.parent_id, "parent");
        assert_eq!(item.title, "Test Track");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "00:00:00");
        assert_eq!(format_duration(90), "00:01:30");
        assert_eq!(format_duration(3665), "01:01:05");
    }
}
