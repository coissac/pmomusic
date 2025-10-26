//! # pmoplaylist - FIFO Audio Universelle pour MediaServer UPnP/OpenHome
//!
//! Cette crate fournit une abstraction de playlist/container audio avec :
//! - Gestion de FIFO audio avec capacité configurable
//! - Exposition d'objets DIDL-Lite via `pmodidl`
//! - Support update_id et last_change pour signaler les modifications
//! - Image par défaut pour le container racine
//!
//! # Exemples
//!
//! ```
//! use pmoplaylist::{FifoPlaylist, Track};
//!
//! # #[tokio::main]
//! # async fn main() {
//! // Créer une FIFO avec capacité de 10 tracks
//! let mut playlist = FifoPlaylist::new(
//!     "radio-1".to_string(),
//!     "Ma Radio Préférée".to_string(),
//!     10,
//!     pmoplaylist::DEFAULT_IMAGE,
//! );
//!
//! // Ajouter un track
//! let track = Track {
//!     id: "track-1".to_string(),
//!     title: "Bohemian Rhapsody".to_string(),
//!     artist: Some("Queen".to_string()),
//!     album: Some("A Night at the Opera".to_string()),
//!     duration: Some(354),
//!     uri: "http://example.com/song.mp3".to_string(),
//!     image: None,
//! };
//!
//! playlist.append_track(track).await;
//!
//! // Récupérer les items pour ContentDirectory
//! let items = playlist.get_items(0, 10).await;
//! println!("Nombre de tracks: {}", items.len());
//!
//! // Générer le container DIDL-Lite
//! let container = playlist.as_container().await;
//! println!("Container ID: {}", container.id);
//! # }
//! ```

use pmodidl::{Container, Item, Resource};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

/// Image WebP par défaut embarquée (1x1 pixel transparent)
/// Remplacez ceci par votre propre image WebP si nécessaire
pub const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/default.webp");

/// Représente un track audio dans la FIFO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Identifiant unique du track
    pub id: String,

    /// Titre du track
    pub title: String,

    /// Artiste (optionnel)
    pub artist: Option<String>,

    /// Album (optionnel)
    pub album: Option<String>,

    /// Durée en secondes (optionnel)
    pub duration: Option<u32>,

    /// URI du flux ou fichier audio
    pub uri: String,

    /// URL de l'image/cover (optionnel, utilise l'image par défaut de la FIFO si absent)
    pub image: Option<String>,
}

impl Track {
    /// Crée un nouveau track avec les informations minimales
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoplaylist::Track;
    ///
    /// let track = Track::new(
    ///     "track-1",
    ///     "Bohemian Rhapsody",
    ///     "http://example.com/song.mp3"
    /// );
    /// ```
    pub fn new(id: impl Into<String>, title: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            artist: None,
            album: None,
            duration: None,
            uri: uri.into(),
            image: None,
        }
    }

    /// Définit l'artiste du track
    pub fn with_artist(mut self, artist: impl Into<String>) -> Self {
        self.artist = Some(artist.into());
        self
    }

    /// Définit l'album du track
    pub fn with_album(mut self, album: impl Into<String>) -> Self {
        self.album = Some(album.into());
        self
    }

    /// Définit la durée du track en secondes
    pub fn with_duration(mut self, duration: u32) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Définit l'URL de l'image du track
    pub fn with_image(mut self, image: impl Into<String>) -> Self {
        self.image = Some(image.into());
        self
    }

    /// Convertit le track en Item DIDL-Lite
    ///
    /// # Arguments
    ///
    /// * `parent_id` - ID du container parent
    /// * `default_image` - Image par défaut si le track n'en a pas
    fn to_didl_item(&self, parent_id: &str, default_image: Option<&str>) -> Item {
        // Formater la durée au format H:MM:SS
        let duration_str = self.duration.map(|d| {
            let hours = d / 3600;
            let minutes = (d % 3600) / 60;
            let seconds = d % 60;
            format!("{}:{:02}:{:02}", hours, minutes, seconds)
        });

        // Utiliser l'image du track ou l'image par défaut
        let album_art = self.image.as_deref().or(default_image).map(String::from);

        // Créer la ressource audio
        let resource = Resource {
            protocol_info: "http-get:*:audio/*:*".to_string(),
            bits_per_sample: None,
            sample_frequency: None,
            nr_audio_channels: None,
            duration: duration_str,
            url: self.uri.clone(),
        };

        Item {
            id: self.id.clone(),
            parent_id: parent_id.to_string(),
            restricted: Some("1".to_string()),
            title: self.title.clone(),
            creator: self.artist.clone(),
            class: "object.item.audioItem.musicTrack".to_string(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            genre: None,
            album_art,
            album_art_pk: None,
            date: None,
            original_track_number: None,
            resources: vec![resource],
            descriptions: vec![],
        }
    }
}

/// FIFO playlist thread-safe avec capacité configurable
#[derive(Clone)]
pub struct FifoPlaylist {
    inner: Arc<RwLock<FifoPlaylistInner>>,
}

struct FifoPlaylistInner {
    /// Identifiant unique de la FIFO
    id: String,

    /// Titre de la FIFO
    title: String,

    /// Image par défaut (WebP embarquée)
    default_image: &'static [u8],

    /// Capacité maximale de la FIFO
    capacity: usize,

    /// Queue FIFO des tracks
    queue: VecDeque<Track>,

    /// Numéro de version pour signaler les modifications
    update_id: u32,

    /// Timestamp de la dernière modification
    last_change: SystemTime,
}

impl FifoPlaylist {
    /// Crée une nouvelle FIFO playlist
    ///
    /// # Arguments
    ///
    /// * `id` - Identifiant unique de la playlist
    /// * `title` - Titre de la playlist
    /// * `capacity` - Capacité maximale (nombre de tracks)
    /// * `default_image` - Image par défaut en format WebP
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoplaylist::FifoPlaylist;
    ///
    /// let playlist = FifoPlaylist::new(
    ///     "radio-1".to_string(),
    ///     "Ma Radio".to_string(),
    ///     10,
    ///     pmoplaylist::DEFAULT_IMAGE,
    /// );
    /// ```
    pub fn new(id: String, title: String, capacity: usize, default_image: &'static [u8]) -> Self {
        Self {
            inner: Arc::new(RwLock::new(FifoPlaylistInner {
                id,
                title,
                default_image,
                capacity,
                queue: VecDeque::new(),
                update_id: 0,
                last_change: SystemTime::now(),
            })),
        }
    }

    /// Ajoute un track à la fin de la FIFO
    ///
    /// Si la capacité est atteinte, le track le plus ancien est supprimé automatiquement.
    /// Met à jour `update_id` et `last_change`.
    ///
    /// # Arguments
    ///
    /// * `track` - Le track à ajouter
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoplaylist::{FifoPlaylist, Track};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut playlist = FifoPlaylist::new(
    ///     "playlist-1".to_string(),
    ///     "My Playlist".to_string(),
    ///     5,
    ///     pmoplaylist::DEFAULT_IMAGE,
    /// );
    ///
    /// let track = Track::new("track-1", "Song Title", "http://example.com/song.mp3");
    /// playlist.append_track(track).await;
    /// # }
    /// ```
    pub async fn append_track(&self, track: Track) {
        let mut inner = self.inner.write().await;

        // Si la capacité est atteinte, supprimer le plus ancien
        if inner.queue.len() >= inner.capacity {
            inner.queue.pop_front();
        }

        inner.queue.push_back(track);
        inner.update_id = inner.update_id.wrapping_add(1);
        inner.last_change = SystemTime::now();
    }

    /// Supprime le track le plus ancien de la FIFO
    ///
    /// Met à jour `update_id` et `last_change` si un track est supprimé.
    /// Retourne le track supprimé, ou None si la FIFO est vide.
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoplaylist::{FifoPlaylist, Track};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut playlist = FifoPlaylist::new(
    ///     "playlist-1".to_string(),
    ///     "My Playlist".to_string(),
    ///     5,
    ///     pmoplaylist::DEFAULT_IMAGE,
    /// );
    ///
    /// playlist.append_track(Track::new("track-1", "Song", "http://example.com/1.mp3")).await;
    ///
    /// let removed = playlist.remove_oldest().await;
    /// assert!(removed.is_some());
    /// # }
    /// ```
    pub async fn remove_oldest(&self) -> Option<Track> {
        let mut inner = self.inner.write().await;

        let track = inner.queue.pop_front();

        if track.is_some() {
            inner.update_id = inner.update_id.wrapping_add(1);
            inner.last_change = SystemTime::now();
        }

        track
    }

    /// Supprime un track par son ID
    ///
    /// Met à jour `update_id` et `last_change` si un track est supprimé.
    /// Retourne true si un track a été supprimé, false sinon.
    ///
    /// # Arguments
    ///
    /// * `track_id` - L'ID du track à supprimer
    pub async fn remove_by_id(&self, track_id: &str) -> bool {
        let mut inner = self.inner.write().await;

        if let Some(pos) = inner.queue.iter().position(|t| t.id == track_id) {
            inner.queue.remove(pos);
            inner.update_id = inner.update_id.wrapping_add(1);
            inner.last_change = SystemTime::now();
            true
        } else {
            false
        }
    }

    /// Vide complètement la FIFO
    ///
    /// Met à jour `update_id` et `last_change` si la FIFO n'était pas vide.
    pub async fn clear(&self) {
        let mut inner = self.inner.write().await;

        if !inner.queue.is_empty() {
            inner.queue.clear();
            inner.update_id = inner.update_id.wrapping_add(1);
            inner.last_change = SystemTime::now();
        }
    }

    /// Retourne le nombre de tracks dans la FIFO
    pub async fn len(&self) -> usize {
        let inner = self.inner.read().await;
        inner.queue.len()
    }

    /// Vérifie si un track existe déjà dans la playlist
    pub async fn has_track(&self, track_id: &str) -> bool {
        let inner = self.inner.read().await;
        inner.queue.iter().any(|t| t.id == track_id)
    }

    /// Met à jour un track existant en appliquant une fonction de mise à jour.
    ///
    /// Retourne `true` si le track a été trouvé et modifié.
    pub async fn update_track<F>(&self, track_id: &str, updater: F) -> bool
    where
        F: FnOnce(&mut Track),
    {
        let mut inner = self.inner.write().await;

        if let Some(track) = inner.queue.iter_mut().find(|t| t.id == track_id) {
            updater(track);
            inner.update_id = inner.update_id.wrapping_add(1);
            inner.last_change = SystemTime::now();
            true
        } else {
            false
        }
    }

    /// Vérifie si la FIFO est vide
    pub async fn is_empty(&self) -> bool {
        let inner = self.inner.read().await;
        inner.queue.is_empty()
    }

    /// Récupère une portion des tracks pour navigation partielle
    ///
    /// # Arguments
    ///
    /// * `offset` - Index de départ (0-based)
    /// * `count` - Nombre maximum de tracks à retourner
    ///
    /// # Retourne
    ///
    /// Un vecteur de tracks, potentiellement vide si offset est hors limite
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoplaylist::{FifoPlaylist, Track};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut playlist = FifoPlaylist::new(
    ///     "playlist-1".to_string(),
    ///     "My Playlist".to_string(),
    ///     10,
    ///     pmoplaylist::DEFAULT_IMAGE,
    /// );
    ///
    /// // Ajouter plusieurs tracks...
    /// for i in 0..5 {
    ///     playlist.append_track(Track::new(
    ///         format!("track-{}", i),
    ///         format!("Song {}", i),
    ///         format!("http://example.com/{}.mp3", i)
    ///     )).await;
    /// }
    ///
    /// // Récupérer les tracks 2 à 4
    /// let items = playlist.get_items(2, 2).await;
    /// assert_eq!(items.len(), 2);
    /// # }
    /// ```
    pub async fn get_items(&self, offset: usize, count: usize) -> Vec<Track> {
        let inner = self.inner.read().await;

        inner
            .queue
            .iter()
            .skip(offset)
            .take(count)
            .cloned()
            .collect()
    }

    /// Retourne l'update_id actuel
    ///
    /// L'update_id est incrémenté à chaque modification de la FIFO.
    /// Utile pour détecter les changements côté client UPnP.
    pub async fn update_id(&self) -> u32 {
        let inner = self.inner.read().await;
        inner.update_id
    }

    /// Retourne le timestamp de la dernière modification
    pub async fn last_change(&self) -> SystemTime {
        let inner = self.inner.read().await;
        inner.last_change
    }

    /// Retourne l'ID de la playlist
    pub async fn id(&self) -> String {
        let inner = self.inner.read().await;
        inner.id.clone()
    }

    /// Retourne le titre de la playlist
    pub async fn title(&self) -> String {
        let inner = self.inner.read().await;
        inner.title.clone()
    }

    /// Génère un Container DIDL-Lite représentant cette FIFO
    ///
    /// Le container peut être utilisé pour le ContentDirectory UPnP.
    ///
    /// # Arguments
    ///
    /// * `parent_id` - ID du container parent (par défaut "0" pour la racine)
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoplaylist::FifoPlaylist;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let playlist = FifoPlaylist::new(
    ///     "radio-1".to_string(),
    ///     "Ma Radio".to_string(),
    ///     10,
    ///     pmoplaylist::DEFAULT_IMAGE,
    /// );
    ///
    /// let container = playlist.as_container_with_parent("0").await;
    /// println!("Container: {:?}", container);
    /// # }
    /// ```
    pub async fn as_container_with_parent(&self, parent_id: impl Into<String>) -> Container {
        let inner = self.inner.read().await;

        Container {
            id: inner.id.clone(),
            parent_id: parent_id.into(),
            restricted: Some("1".to_string()),
            child_count: Some(inner.queue.len().to_string()),
            searchable: Some("1".to_string()),
            title: inner.title.clone(),
            class: "object.container.playlistContainer".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Génère un Container DIDL-Lite avec parent_id = "0"
    pub async fn as_container(&self) -> Container {
        self.as_container_with_parent("0").await
    }

    /// Génère un vecteur d'objets DIDL-Lite Item correspondant aux tracks
    ///
    /// # Arguments
    ///
    /// * `offset` - Index de départ (0-based)
    /// * `count` - Nombre maximum d'items à retourner
    /// * `default_image_url` - URL optionnelle pour l'image par défaut (endpoint servant l'image)
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoplaylist::{FifoPlaylist, Track};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut playlist = FifoPlaylist::new(
    ///     "radio-1".to_string(),
    ///     "Ma Radio".to_string(),
    ///     10,
    ///     pmoplaylist::DEFAULT_IMAGE,
    /// );
    ///
    /// playlist.append_track(Track::new("track-1", "Song", "http://example.com/1.mp3")).await;
    ///
    /// let items = playlist.as_objects(0, 10, Some("http://server/default.webp")).await;
    /// assert_eq!(items.len(), 1);
    /// # }
    /// ```
    pub async fn as_objects(
        &self,
        offset: usize,
        count: usize,
        default_image_url: Option<&str>,
    ) -> Vec<Item> {
        let inner = self.inner.read().await;

        inner
            .queue
            .iter()
            .skip(offset)
            .take(count)
            .map(|track| track.to_didl_item(&inner.id, default_image_url))
            .collect()
    }

    /// Retourne l'image par défaut en tant que slice de bytes
    ///
    /// Peut être servi via un endpoint HTTP pour les clients UPnP
    pub async fn default_image(&self) -> &'static [u8] {
        let inner = self.inner.read().await;
        inner.default_image
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_playlist() {
        let playlist = FifoPlaylist::new(
            "test-1".to_string(),
            "Test Playlist".to_string(),
            5,
            DEFAULT_IMAGE,
        );

        assert_eq!(playlist.len().await, 0);
        assert!(playlist.is_empty().await);
        assert_eq!(playlist.update_id().await, 0);
    }

    #[tokio::test]
    async fn test_append_track() {
        let playlist = FifoPlaylist::new(
            "test-1".to_string(),
            "Test Playlist".to_string(),
            5,
            DEFAULT_IMAGE,
        );

        let track = Track::new("track-1", "Song 1", "http://example.com/1.mp3");
        playlist.append_track(track).await;

        assert_eq!(playlist.len().await, 1);
        assert_eq!(playlist.update_id().await, 1);
    }

    #[tokio::test]
    async fn test_fifo_capacity() {
        let playlist = FifoPlaylist::new(
            "test-1".to_string(),
            "Test Playlist".to_string(),
            3,
            DEFAULT_IMAGE,
        );

        // Ajouter 5 tracks alors que la capacité est 3
        for i in 0..5 {
            let track = Track::new(
                format!("track-{}", i),
                format!("Song {}", i),
                format!("http://example.com/{}.mp3", i),
            );
            playlist.append_track(track).await;
        }

        // Seuls les 3 derniers doivent rester
        assert_eq!(playlist.len().await, 3);

        let items = playlist.get_items(0, 10).await;
        assert_eq!(items[0].id, "track-2");
        assert_eq!(items[2].id, "track-4");
    }

    #[tokio::test]
    async fn test_remove_oldest() {
        let playlist = FifoPlaylist::new(
            "test-1".to_string(),
            "Test Playlist".to_string(),
            5,
            DEFAULT_IMAGE,
        );

        playlist
            .append_track(Track::new("track-1", "Song 1", "http://example.com/1.mp3"))
            .await;
        playlist
            .append_track(Track::new("track-2", "Song 2", "http://example.com/2.mp3"))
            .await;

        let removed = playlist.remove_oldest().await;
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, "track-1");
        assert_eq!(playlist.len().await, 1);
    }

    #[tokio::test]
    async fn test_remove_by_id() {
        let playlist = FifoPlaylist::new(
            "test-1".to_string(),
            "Test Playlist".to_string(),
            5,
            DEFAULT_IMAGE,
        );

        playlist
            .append_track(Track::new("track-1", "Song 1", "http://example.com/1.mp3"))
            .await;
        playlist
            .append_track(Track::new("track-2", "Song 2", "http://example.com/2.mp3"))
            .await;
        playlist
            .append_track(Track::new("track-3", "Song 3", "http://example.com/3.mp3"))
            .await;

        assert!(playlist.remove_by_id("track-2").await);
        assert_eq!(playlist.len().await, 2);

        let items = playlist.get_items(0, 10).await;
        assert_eq!(items[0].id, "track-1");
        assert_eq!(items[1].id, "track-3");
    }

    #[tokio::test]
    async fn test_clear() {
        let playlist = FifoPlaylist::new(
            "test-1".to_string(),
            "Test Playlist".to_string(),
            5,
            DEFAULT_IMAGE,
        );

        playlist
            .append_track(Track::new("track-1", "Song 1", "http://example.com/1.mp3"))
            .await;
        playlist
            .append_track(Track::new("track-2", "Song 2", "http://example.com/2.mp3"))
            .await;

        playlist.clear().await;
        assert_eq!(playlist.len().await, 0);
        assert!(playlist.is_empty().await);
    }

    #[tokio::test]
    async fn test_get_items_pagination() {
        let playlist = FifoPlaylist::new(
            "test-1".to_string(),
            "Test Playlist".to_string(),
            10,
            DEFAULT_IMAGE,
        );

        for i in 0..5 {
            playlist
                .append_track(Track::new(
                    format!("track-{}", i),
                    format!("Song {}", i),
                    format!("http://example.com/{}.mp3", i),
                ))
                .await;
        }

        let items = playlist.get_items(1, 2).await;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "track-1");
        assert_eq!(items[1].id, "track-2");
    }

    #[tokio::test]
    async fn test_as_container() {
        let playlist = FifoPlaylist::new(
            "radio-1".to_string(),
            "Test Radio".to_string(),
            10,
            DEFAULT_IMAGE,
        );

        playlist
            .append_track(Track::new("track-1", "Song 1", "http://example.com/1.mp3"))
            .await;

        let container = playlist.as_container().await;
        assert_eq!(container.id, "radio-1");
        assert_eq!(container.title, "Test Radio");
        assert_eq!(container.parent_id, "0");
        assert_eq!(container.child_count, Some("1".to_string()));
    }

    #[tokio::test]
    async fn test_as_objects() {
        let playlist = FifoPlaylist::new(
            "radio-1".to_string(),
            "Test Radio".to_string(),
            10,
            DEFAULT_IMAGE,
        );

        let track = Track::new(
            "track-1",
            "Bohemian Rhapsody",
            "http://example.com/song.mp3",
        )
        .with_artist("Queen")
        .with_album("A Night at the Opera")
        .with_duration(354);

        playlist.append_track(track).await;

        let items = playlist
            .as_objects(0, 10, Some("http://server/default.webp"))
            .await;
        assert_eq!(items.len(), 1);

        let item = &items[0];
        assert_eq!(item.id, "track-1");
        assert_eq!(item.title, "Bohemian Rhapsody");
        assert_eq!(item.artist, Some("Queen".to_string()));
        assert_eq!(item.album, Some("A Night at the Opera".to_string()));
        assert_eq!(item.parent_id, "radio-1");
        assert!(item.resources.len() > 0);
    }

    #[tokio::test]
    async fn test_track_builder() {
        let track = Track::new("track-1", "Song", "http://example.com/song.mp3")
            .with_artist("Artist")
            .with_album("Album")
            .with_duration(180)
            .with_image("http://example.com/cover.jpg");

        assert_eq!(track.artist, Some("Artist".to_string()));
        assert_eq!(track.album, Some("Album".to_string()));
        assert_eq!(track.duration, Some(180));
        assert_eq!(
            track.image,
            Some("http://example.com/cover.jpg".to_string())
        );
    }

    #[tokio::test]
    async fn test_update_id_increments() {
        let playlist = FifoPlaylist::new(
            "test-1".to_string(),
            "Test Playlist".to_string(),
            5,
            DEFAULT_IMAGE,
        );

        assert_eq!(playlist.update_id().await, 0);

        playlist
            .append_track(Track::new("track-1", "Song 1", "http://example.com/1.mp3"))
            .await;
        assert_eq!(playlist.update_id().await, 1);

        playlist
            .append_track(Track::new("track-2", "Song 2", "http://example.com/2.mp3"))
            .await;
        assert_eq!(playlist.update_id().await, 2);

        playlist.remove_oldest().await;
        assert_eq!(playlist.update_id().await, 3);

        playlist.clear().await;
        assert_eq!(playlist.update_id().await, 4);
    }
}
