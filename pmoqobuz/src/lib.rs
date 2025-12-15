//! # pmoqobuz - Client Qobuz pour PMOMusic
//!
//! Cette crate fournit un client Rust pour l'API Qobuz, inspiré de l'implémentation Python d'upmpdcli,
//! avec un système de cache en mémoire et une intégration avec les autres modules PMOMusic.
//!
//! ## Vue d'ensemble
//!
//! `pmoqobuz` permet d'accéder aux fonctionnalités de Qobuz :
//! - Authentification avec les credentials configurés
//! - Navigation dans le catalogue (albums, artistes, playlists, tracks)
//! - Recherche dans le catalogue
//! - Accès aux favoris de l'utilisateur
//! - Cache en mémoire pour minimiser les requêtes API
//! - Export des objets en format DIDL-Lite (via `pmodidl`)
//! - Cache des images d'albums (via `pmocovers`)
//!
//! ## Architecture
//!
//! La crate suit le pattern d'extension des autres crates PMO :
//! - `QobuzClient` : Client principal avec authentification et cache
//! - `models` : Structures de données (Album, Track, Artist, etc.)
//! - `api` : Couche d'accès à l'API REST Qobuz
//! - `cache` : Système de cache en mémoire avec TTL
//! - `didl` : Export des objets en format DIDL-Lite
//!
//! ## Structure des modules
//!
//! ```text
//! pmoqobuz/
//! ├── src/
//! │   ├── lib.rs              # Module principal (ce fichier)
//! │   ├── client.rs           # Client Qobuz principal
//! │   ├── models.rs           # Structures de données
//! │   ├── api/
//! │   │   ├── mod.rs          # API client
//! │   │   ├── auth.rs         # Authentification
//! │   │   ├── catalog.rs      # Accès au catalogue
//! │   │   └── user.rs         # API utilisateur (favoris)
//! │   ├── cache.rs            # Cache en mémoire
//! │   ├── didl.rs             # Export DIDL-Lite
//! │   └── error.rs            # Gestion des erreurs
//! ```
//!
//! ## Utilisation
//!
//! ### Exemple basique avec configuration automatique
//!
//! ```rust,no_run
//! use pmoqobuz::{QobuzClient, ToDIDL};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Utilise automatiquement la config depuis pmoconfig
//!     let client = QobuzClient::from_config().await?;
//!
//!     // Rechercher des albums
//!     let results = client.search_albums("Miles Davis").await?;
//!     for album in results {
//!         println!("{} - {}", album.artist.name, album.title);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Exemple avec credentials personnalisés
//!
//! ```rust,no_run
//! use pmoqobuz::{QobuzClient, ToDIDL};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = QobuzClient::new("user@example.com", "password").await?;
//!
//!     // Obtenir les albums favoris
//!     let favorites = client.get_favorite_albums().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Export DIDL-Lite
//!
//! ```rust,no_run
//! use pmoqobuz::{QobuzClient, ToDIDL};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = QobuzClient::from_config().await?;
//!
//!     let album = client.get_album("12345").await?;
//!     let didl_container = album.to_didl_container("parent_id")?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Cache
//!
//! Le client utilise un cache en mémoire avec TTL pour minimiser les requêtes à l'API Qobuz :
//! - Albums : 1 heure
//! - Tracks : 1 heure
//! - Artistes : 1 heure
//! - Playlists : 30 minutes
//! - Résultats de recherche : 15 minutes
//! - URLs de streaming : 5 minutes
//!
//! ## Intégration pmocovers et pmoaudiocache
//!
//! La feature `cache` active le support complet du cache pour les images et l'audio.
//!
//! ### Cache d'images (pmocovers)
//!
//! Les images de couverture sont automatiquement téléchargées et converties en WebP :
//!
//! ```rust,no_run
//! use pmoqobuz::{QobuzSource, QobuzClient};
//! use pmoaudiocache::Cache as AudioCache;
//! use pmocovers::Cache as CoverCache;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = QobuzClient::from_config().await?;
//! let cover_cache = Arc::new(CoverCache::new("./cache/covers", 500)?);
//! let audio_cache = Arc::new(AudioCache::new("./cache/audio", 100)?);
//!
//! let source = QobuzSource::new(client, cover_cache, audio_cache);
//! # Ok(())
//! # }
//! ```
//!
//! ### Cache audio (pmoaudiocache)
//!
//! L'audio haute résolution est téléchargé et caché localement avec métadonnées enrichies :
//!
//! ```rust,no_run
//! use pmoqobuz::{QobuzSource, QobuzClient};
//! use pmocovers::Cache as CoverCache;
//! use pmoaudiocache::Cache as AudioCache;
//! use pmosource::MusicSource;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = QobuzClient::from_config().await?;
//! let cover_cache = Arc::new(CoverCache::new("./cache/covers", 500)?);
//! let audio_cache = Arc::new(AudioCache::new("./cache/audio", 100)?);
//!
//! let source = QobuzSource::new(client, cover_cache, audio_cache);
//!
//! // Add a track with caching
//! let tracks = source.client().get_favorite_tracks().await?;
//! if let Some(track) = tracks.first() {
//!     let track_id = source.add_track(track).await?;
//!     // Audio and cover are now cached with rich metadata
//!
//!     // Resolve URI (returns cached version if available)
//!     let uri = source.resolve_uri(&track_id).await?;
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### Métadonnées enrichies
//!
//! Qobuz fournit des métadonnées détaillées qui sont préservées dans le cache :
//! - Titre, artiste, album
//! - Numéro de piste et de disque
//! - Année de sortie
//! - Genre(s)
//! - Label
//! - Qualité audio (sample rate, bit depth, channels)
//! - Durée
//!
//! ### Exemple complet
//!
//! Voir `examples/with_cache.rs` pour un exemple complet d'utilisation avec cache.
//!
//! ## Formats audio supportés
//!
//! Qobuz propose plusieurs formats :
//! - Format 5 : MP3 320 kbps
//! - Format 6 : FLAC 16 bit / 44.1 kHz (CD Quality)
//! - Format 7 : FLAC 24 bit / jusqu'à 96 kHz (Hi-Res)
//! - Format 27 : FLAC 24 bit / jusqu'à 192 kHz (Hi-Res+)
//!
//! ## Gestion des erreurs
//!
//! La crate utilise `thiserror` pour définir des erreurs typées :
//!
//! ```rust,ignore
//! use pmoqobuz::{QobuzClient, QobuzError};
//!
//! match client.get_album("invalid").await {
//!     Ok(album) => println!("Album: {}", album.title),
//!     Err(QobuzError::NotFound) => println!("Album not found"),
//!     Err(QobuzError::Unauthorized) => println!("Authentication failed"),
//!     Err(e) => println!("Error: {}", e),
//! }
//! ```
//!
//! ## Voir aussi
//!
//! - [`pmodidl`] : Format DIDL-Lite
//! - [`pmocovers`] : Cache d'images
//! - [`pmoaudiocache`] : Cache audio
//! - [`pmoconfig`] : Configuration
//! - [`pmoserver`] : Serveur HTTP

pub mod api;
pub mod cache;
pub mod client;
pub mod config_ext;
pub mod didl;
#[cfg(feature = "disk-cache")]
pub mod disk_cache;
pub mod error;
mod lazy_provider;
pub mod models;
pub mod source;

// Extension pmoserver (feature-gated)
#[cfg(feature = "pmoserver")]
pub mod api_rest;

#[cfg(feature = "pmoserver")]
pub mod pmoserver_ext;

#[cfg(feature = "pmoserver")]
mod pmoserver_impl;

pub use client::QobuzClient;
pub use config_ext::QobuzConfigExt;
pub use error::{QobuzError, Result};
pub use models::{Album, Artist, AudioFormat, Genre, Playlist, SearchResult, Track};
pub use source::QobuzSource;

/// Ré-exporte les types DIDL pour faciliter l'utilisation
pub use didl::ToDIDL;

/// Ré-exporte le trait d'extension pmoserver
#[cfg(feature = "pmoserver")]
pub use pmoserver_ext::QobuzServerExt;
