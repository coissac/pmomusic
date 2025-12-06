//! # pmocache – Système de cache générique pour PMOMusic
//!
//! Cette crate fournit les briques communes utilisées par les caches de PMOMusic.
//! Elle gère l'association entre fichiers stockés sur disque et métadonnées
//! conservées dans une base SQLite, ainsi que les opérations de téléchargement,
//! d'éviction et de mise à jour.
//!
//! ## Vue d'ensemble
//!
//! `pmocache` met à disposition :
//! - un modèle `Cache` asynchrone pour stocker des fichiers et leurs métadonnées ;
//! - un module `db` encapsulant l'accès SQLite (table `asset` + table `metadata`) ;
//! - des utilitaires de téléchargement (`download`) réutilisables par les caches spécialisés ;
//! - un trait `CacheConfig` permettant de paramétrer l'extension, le nom et le type du cache.
//!
//! Les crates `pmocovers` (images) et `pmoaudiocache` (pistes audio) s'appuient sur ces
//! composants et ajoutent leurs propres contraintes métier (conversion WebP, métadonnées audio…).
//!
//! ## Exemple basique
//!
//! ```rust,no_run
//! use pmocache::{Cache, CacheConfig};
//!
//! struct MyConfig;
//! impl CacheConfig for MyConfig {
//!     fn file_extension() -> &'static str { "dat" }
//! }
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let cache = Cache::<MyConfig>::new("./cache", 1000)?;
//!
//!    // Ajout d'un fichier depuis une URL
//!     let pk = cache.add_from_url("https://example.com/file.dat", None).await?;
//!     println!("Fichier ajouté avec la clé {pk}");
//!
//!     // Récupération du fichier local
//!     let path = cache.get(&pk).await?;
//!     println!("Fichier disponible à {path:?}");
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Collections
//!
//! ```rust,no_run
//! use pmocache::{Cache, CacheConfig};
//!
//! struct AudioConfig;
//! impl CacheConfig for AudioConfig {
//!     fn file_extension() -> &'static str { "flac" }
//!     fn cache_type() -> &'static str { "audio" }
//!     fn cache_name() -> &'static str { "tracks" }
//! }
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let cache = Cache::<AudioConfig>::new("./audio-cache", 200)?;
//!
//!     let album = "album:the_wall";
//!     cache.add_from_url("https://example.com/track1.flac", Some(album)).await?;
//!     cache.add_from_url("https://example.com/track2.flac", Some(album)).await?;
//!
//!     let files = cache.get_collection(album).await?;
//!     println!("Album {album} : {} fichiers en cache", files.len());
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Structure sur disque
//!
//! ```text
//! cache/
//! ├── cache.db            # Base SQLite
//! ├── 1a2b3c4d.orig.dat   # Fichier original
//! └── 1a2b3c4d.thumb.dat  # Variante (qualifier différent)
//! ```
//!
//! Les métadonnées sont conservées dans deux tables :
//!
//! ```sql
//! CREATE TABLE asset (
//!     pk TEXT PRIMARY KEY,
//!     collection TEXT,
//!     id TEXT,
//!     hits INTEGER DEFAULT 0,
//!     last_used TEXT
//! );
//!
//! CREATE TABLE metadata (
//!     pk TEXT,
//!     key TEXT,
//!     value_type TEXT CHECK(value_type IN ('string','number','boolean','null')),
//!     value TEXT,
//!     PRIMARY KEY (pk, key),
//!     FOREIGN KEY (pk) REFERENCES asset(pk) ON DELETE CASCADE
//! );
//! ```
//!
//! ## Modules principaux
//!
//! - [`cache`] : gestion du cache sur disque + opérations asynchrones ;
//! - [`db`] : accès SQLite, contraintes et helpers métadonnées ;
//! - [`download`] : primitives de téléchargement et de transformation ;
//! - [`cache_trait`] : trait partagé entre implémentations spécialisées.
//!
//! ## Crates associées
//!
//! - [`pmocovers`] : cache d'images reposant sur `pmocache` ;
//! - [`pmoaudiocache`] : spécialisation audio avec extraction de métadonnées.

pub mod cache;
pub mod cache_trait;
pub mod db;
pub mod download;
pub mod metadata_macros;

#[cfg(feature = "pmoserver")]
pub mod pmoserver_ext;

#[cfg(feature = "pmoserver")]
pub mod api;

#[cfg(feature = "openapi")]
pub mod openapi;

#[cfg(feature = "pmoconfig")]
pub mod config_ext;

pub use cache::{Cache, CacheBroadcastEvent, CacheConfig, CacheSubscription};
pub use cache_trait::{pk_from_content_header, FileCache};
pub use db::{CacheEntry, DB};
pub use download::{
    download, download_with_transformer, ingest_with_transformer, peek_header, peek_reader_header,
    Download, StreamTransformer, TransformContextHandle, TransformMetadata,
};

#[cfg(feature = "pmoserver")]
pub use pmoserver_ext::{create_api_router, create_file_router, GenericCacheExt};

#[cfg(all(feature = "pmoserver", feature = "openapi"))]
pub use api::{AddItemRequest, AddItemResponse, DeleteItemResponse, DownloadStatus, ErrorResponse};

#[cfg(feature = "pmoconfig")]
pub use config_ext::CacheConfigExt;
