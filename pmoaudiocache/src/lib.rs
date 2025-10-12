//! # pmoaudiocache - Cache de pistes audio pour PMOMusic
//!
//! Cette crate fournit un système de cache pour les pistes audio avec extraction
//! automatique des métadonnées et gestion de collections (albums).
//!
//! ## Vue d'ensemble
//!
//! `pmoaudiocache` étend `pmocache` pour gérer spécifiquement les fichiers audio :
//! - Téléchargement et stockage de pistes audio
//! - Extraction automatique des métadonnées (titre, artiste, album, etc.)
//! - Gestion de collections basées sur artiste/album
//! - Cache persistant avec base de données SQLite
//! - API HTTP optionnelle pour récupérer les pistes
//!
//! ## Fonctionnalités
//!
//! ### 📦 Gestion du cache
//! - Téléchargement automatique depuis des URLs
//! - **Conversion automatique en FLAC** (standardisation du stockage)
//! - Stockage persistant sur disque
//! - Base de données SQLite pour le tracking
//! - Extraction des métadonnées audio (via lofty)
//!
//! ### 🎵 Gestion des collections
//! - Regroupement automatique par artiste/album
//! - Tri par numéro de piste
//! - Liste des collections disponibles
//! - Récupération de tous les tracks d'un album
//!
//! ### 📊 Statistiques d'utilisation
//! - Comptage des accès (hits)
//! - Suivi de la dernière utilisation
//! - API de statistiques complètes
//!
//! ## Architecture
//!
//! `pmoaudiocache` utilise `pmocache` comme base :
//!
//! ```text
//! pmoaudiocache/
//! ├── Cargo.toml
//! ├── src/
//! │   ├── lib.rs              # Module principal (ce fichier)
//! │   ├── cache.rs            # Gestion du cache audio
//! │   ├── metadata.rs         # Extraction de métadonnées
//! │   └── pmoserver_impl.rs   # Extension de pmoserver::Server (optionnel)
//! └── cache/                  # Répertoire de cache (généré)
//!     ├── cache.db            # Base SQLite
//!     └── *.audio             # Fichiers audio
//! ```
//!
//! ## Utilisation
//!
//! ### Exemple basique
//!
//! ```rust,no_run
//! use pmoaudiocache::AudioCache;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let cache = AudioCache::new("./audio_cache", 1000)?;
//!
//!     // Ajouter une piste depuis une URL
//!     let (pk, metadata) = cache.add_from_url("http://example.com/track.flac").await?;
//!     println!("Piste ajoutée: {} - {}", metadata.artist.unwrap(), metadata.title.unwrap());
//!
//!     // Récupérer la piste
//!     let (path, metadata) = cache.get(&pk).await?;
//!     println!("Piste stockée à: {:?}", path);
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Utilisation avec des collections
//!
//! ```rust,no_run
//! use pmoaudiocache::AudioCache;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let cache = AudioCache::new("./audio_cache", 1000)?;
//!
//!     // Ajouter des pistes (elles seront automatiquement regroupées par album)
//!     cache.add_from_url("http://example.com/track1.flac").await?;
//!     cache.add_from_url("http://example.com/track2.flac").await?;
//!
//!     // Lister les collections disponibles
//!     let collections = cache.list_collections().await?;
//!     for (collection, count) in collections {
//!         println!("Collection: {} ({} pistes)", collection, count);
//!     }
//!
//!     // Récupérer toutes les pistes d'un album
//!     let tracks = cache.get_collection("pink_floyd:wish_you_were_here").await?;
//!     for (pk, path, metadata) in tracks {
//!         println!("{:02}. {} - {}",
//!             metadata.track_number.unwrap_or(0),
//!             metadata.title.unwrap_or_default(),
//!             path.display()
//!         );
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## API HTTP (avec feature "pmoserver")
//!
//! Lorsque la feature `pmoserver` est activée, vous pouvez intégrer le cache audio
//! à un serveur HTTP :
//!
//! ```rust,no_run
//! use pmoaudiocache::AudioCacheExt;
//! use pmoserver::ServerBuilder;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut server = ServerBuilder::new_configured().build();
//!
//!     // Initialiser le cache audio
//!     server.init_audio_cache("./audio_cache", 1000).await?;
//!
//!     server.start().await;
//!     server.wait().await;
//!     Ok(())
//! }
//! ```
//!
//! Les endpoints suivants sont disponibles :
//!
//! - `GET /audio/tracks/{pk}` - Récupère une piste audio
//! - `GET /audio/tracks/{pk}/metadata` - Récupère les métadonnées d'une piste
//! - `GET /audio/collections` - Liste les collections disponibles
//! - `GET /audio/collections/{collection}` - Récupère toutes les pistes d'une collection
//! - `GET /audio/stats` - Statistiques du cache
//!
//! ## Métadonnées supportées
//!
//! Les métadonnées suivantes sont extraites automatiquement :
//!
//! - Titre, artiste, album
//! - Année, genre
//! - Numéro de piste/disque
//! - Durée, taux d'échantillonnage, bitrate
//! - Nombre de canaux
//!
//! ## Format des collections
//!
//! Les collections sont identifiées par une clé au format `"artist:album"`, avec :
//! - Conversion en minuscules
//! - Remplacement des espaces par des underscores
//! - Exemple : `"Pink Floyd - Wish You Were Here"` → `"pink_floyd:wish_you_were_here"`
//!
//! ## Dépendances principales
//!
//! - `pmocache` : Cache générique
//! - `lofty` : Extraction de métadonnées audio
//! - `reqwest` : Téléchargement HTTP
//! - `tokio` : Runtime asynchrone
//!
//! ## Voir aussi
//!
//! - [`pmocache`] : Cache générique
//! - [`pmocovers`] : Cache d'images
//! - [`pmoserver`] : Serveur HTTP

pub mod cache;
pub mod metadata;
pub mod flac;
pub mod db;

pub use cache::AudioCache;
pub use metadata::AudioMetadata;
pub use db::{AudioDB, AudioCacheEntry};

/// Trait pour étendre un serveur HTTP avec des fonctionnalités de cache audio.
///
/// Ce trait permet à `pmoaudiocache` d'ajouter des méthodes d'extension sur des types
/// de serveurs externes (comme `pmoserver::Server`) sans que ces crates dépendent de `pmoaudiocache`.
#[cfg(feature = "pmoserver")]
pub trait AudioCacheExt {
    /// Initialise le cache audio et enregistre les routes HTTP.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Répertoire de stockage du cache
    /// * `limit` - Limite de taille du cache (en nombre de pistes)
    ///
    /// # Returns
    ///
    /// * `Arc<AudioCache>` - Instance partagée du cache
    async fn init_audio_cache(&mut self, cache_dir: &str, limit: usize) -> anyhow::Result<std::sync::Arc<AudioCache>>;

    /// Initialise le cache audio avec la configuration par défaut.
    ///
    /// Utilise automatiquement les paramètres de `pmoconfig::Config`.
    async fn init_audio_cache_configured(&mut self) -> anyhow::Result<std::sync::Arc<AudioCache>>;
}

// Implémentation du trait pour pmoserver::Server (feature-gated)
#[cfg(feature = "pmoserver")]
mod pmoserver_impl;

#[cfg(feature = "pmoserver")]
pub mod api;

#[cfg(feature = "pmoserver")]
pub mod openapi;

#[cfg(feature = "pmoserver")]
pub use openapi::ApiDoc;
