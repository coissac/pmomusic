//! # pmocache - Système de cache générique pour PMOMusic
//!
//! Cette crate fournit un système de cache générique avec support de base de données SQLite
//! et stockage sur disque. Elle est utilisée comme base pour des caches spécialisés comme
//! `pmocovers` (cache d'images) et `pmoaudiocache` (cache de pistes audio).
//!
//! ## Vue d'ensemble
//!
//! `pmocache` fournit les composants de base pour :
//! - Stocker des fichiers sur disque avec une base de données SQLite pour les métadonnées
//! - Gérer des collections d'éléments (albums, playlists, etc.)
//! - Suivre les statistiques d'utilisation (hits, dernière utilisation)
//! - Télécharger automatiquement depuis des URLs
//! - Consolider et purger le cache
//!
//! ## Architecture
//!
//! `pmocache` est conçu comme une base générique :
//!
//! ```text
//! pmocache (générique)
//!     ├── db.rs       - Base de données SQLite générique
//!     └── cache.rs    - Système de cache générique
//!
//! pmocovers (spécialisé pour les images)
//!     └── Utilise pmocache + conversion WebP
//!
//! pmoaudiocache (spécialisé pour l'audio)
//!     └── Utilise pmocache + métadonnées audio
//! ```
//!
//! ## Utilisation
//!
//! ### Exemple basique
//!
//! ```rust,no_run
//! use pmocache::{Cache, CacheConfig};
//!
//! // Définir la configuration du cache
//! struct MyConfig;
//! impl CacheConfig for MyConfig {
//!     fn file_extension() -> &'static str { "dat" }
//!     fn table_name() -> &'static str { "my_cache" }
//!     fn cache_type() -> &'static str { "generic" }
//! }
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let cache = Cache::<MyConfig>::new("./cache", 1000, "http://localhost:8080")?;
//!
//!     // Ajouter un fichier depuis une URL
//!     let pk = cache.add_from_url("http://example.com/file.dat", None).await?;
//!     println!("Fichier ajouté avec clé: {}", pk);
//!
//!     // Récupérer le fichier
//!     let path = cache.get(&pk).await?;
//!     println!("Fichier stocké à: {:?}", path);
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Utilisation avec des collections
//!
//! ```rust,no_run
//! use pmocache::{Cache, CacheConfig};
//!
//! struct AudioConfig;
//! impl CacheConfig for AudioConfig {
//!     fn file_extension() -> &'static str { "flac" }
//!     fn table_name() -> &'static str { "audio" }
//!     fn cache_type() -> &'static str { "audio" }
//! }
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let cache = Cache::<AudioConfig>::new("./cache", 1000, "http://localhost:8080")?;
//!
//!     // Ajouter des pistes d'un album
//!     let album_id = "album:the_wall";
//!     cache.add_from_url("http://example.com/track1.flac", Some(album_id)).await?;
//!     cache.add_from_url("http://example.com/track2.flac", Some(album_id)).await?;
//!
//!     // Récupérer toutes les pistes de l'album
//!     let tracks = cache.get_collection(album_id).await?;
//!     println!("Album contient {} pistes", tracks.len());
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Structure des fichiers
//!
//! ```text
//! cache/
//! ├── cache.db                      # Base de données SQLite
//! ├── 1a2b3c4d.webp                 # Fichier 1
//! └── 5e6f7a8b.flac                 # Fichier 2
//! ```
//!
//! ## Schéma de base de données
//!
//! ```sql
//! CREATE TABLE {table_name} (
//!     pk TEXT PRIMARY KEY,           -- Clé unique (hash SHA1 de l'URL)
//!     source_url TEXT,               -- URL source
//!     collection TEXT,               -- Collection (album, playlist, etc.)
//!     hits INTEGER DEFAULT 0,        -- Nombre d'accès
//!     last_used TEXT                 -- Dernière utilisation (RFC3339)
//! );
//! ```
//!
//! ## Dépendances principales
//!
//! - `rusqlite` : Base de données SQLite
//! - `reqwest` : Téléchargement HTTP
//! - `sha1` : Génération de clés
//! - `tokio` : Runtime asynchrone
//!
//! ## Voir aussi
//!
//! - [`pmocovers`] : Cache d'images avec conversion WebP
//! - [`pmoaudiocache`] : Cache de pistes audio

pub mod db;
pub mod cache;
pub mod cache_trait;
pub mod download;

#[cfg(feature = "pmoserver")]
pub mod pmoserver_ext;

#[cfg(feature = "pmoserver")]
pub mod api;

#[cfg(feature = "openapi")]
pub mod openapi;

pub use db::{DB, CacheEntry};
pub use cache::{Cache, CacheConfig};
pub use cache_trait::{FileCache, pk_from_url};
pub use download::{Download, download, download_with_transformer, StreamTransformer};

#[cfg(feature = "pmoserver")]
pub use pmoserver_ext::{create_file_router, create_api_router, GenericCacheExt};

#[cfg(all(feature = "pmoserver", feature = "openapi"))]
pub use api::{
    DownloadStatus, AddItemRequest, AddItemResponse,
    DeleteItemResponse, ErrorResponse,
};
