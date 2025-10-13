//! # pmoaudiocache - Cache de pistes audio pour PMOMusic
//!
//! Cette crate fournit un système de cache pour les pistes audio avec extraction
//! automatique des métadonnées et gestion de collections (albums).
//!
//! ## Vue d'ensemble
//!
//! `pmoaudiocache` étend `pmocache` pour gérer spécifiquement les fichiers audio :
//! - **Cache à deux phases** : métadonnées immédiates + conversion asynchrone
//! - Téléchargement et stockage de pistes audio
//! - Extraction automatique des métadonnées (fichier + services externes)
//! - Gestion de collections basées sur artiste/album
//! - Cache persistant avec base de données SQLite
//! - API HTTP optionnelle pour récupérer les pistes
//!
//! ## Fonctionnalités principales
//!
//! ### ⚡ Cache à deux phases
//!
//! Le système de cache permet de servir les métadonnées **immédiatement** (< 1 seconde)
//! pendant que la conversion FLAC s'effectue en arrière-plan :
//!
//! **Phase 1 (immédiate)** :
//! - Extraction des métadonnées du fichier original
//! - Fusion avec métadonnées externes (Qobuz, Radio Paradise, CD)
//! - Stockage en base de données
//! - Service immédiat du DIDL-Lite pour MediaServer
//!
//! **Phase 2 (asynchrone)** :
//! - Conversion automatique en FLAC en arrière-plan
//! - Suivi du statut de conversion
//! - Nettoyage automatique des fichiers temporaires
//!
//! ### 📦 Gestion du cache
//! - Téléchargement automatique depuis des URLs
//! - **Conversion automatique en FLAC** (standardisation du stockage)
//! - Stockage persistant sur disque
//! - Base de données SQLite pour le tracking des métadonnées ET du statut
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
//! use pmoaudiocache::{AudioCache, AudioMetadata};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let cache = AudioCache::new("./audio_cache", 1000)?;
//!
//!     // Ajouter une piste depuis une URL (sans métadonnées externes)
//!     let (pk, metadata) = cache.add_from_url("http://example.com/track.flac", None).await?;
//!     println!("Piste ajoutée: {} - {}",
//!              metadata.artist.as_deref().unwrap_or("Unknown"),
//!              metadata.title.as_deref().unwrap_or("Unknown"));
//!
//!     // Les métadonnées sont disponibles IMMÉDIATEMENT
//!     let metadata = cache.get_metadata(&pk).await?;
//!     println!("Métadonnées disponibles: {:?}", metadata);
//!
//!     // Le fichier FLAC est disponible après conversion
//!     let file_path = cache.get_file(&pk).await?;
//!     println!("Fichier FLAC stocké à: {:?}", file_path);
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Exemple avec métadonnées externes (Qobuz, Radio Paradise, etc.)
//!
//! ```rust,no_run
//! use pmoaudiocache::{AudioCache, AudioMetadata};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let cache = AudioCache::new("./audio_cache", 1000)?;
//!
//!     // Métadonnées provenant d'un service externe (Qobuz, etc.)
//!     let external_metadata = AudioMetadata {
//!         title: Some("Wish You Were Here".to_string()),
//!         artist: Some("Pink Floyd".to_string()),
//!         album: Some("Wish You Were Here".to_string()),
//!         year: Some(1975),
//!         track_number: Some(1),
//!         ..Default::default()
//!     };
//!
//!     // Ajouter la piste avec fusion des métadonnées
//!     // (les métadonnées externes ont priorité sur celles du fichier)
//!     let (pk, metadata) = cache.add_from_url(
//!         "http://example.com/track.flac",
//!         Some(external_metadata)
//!     ).await?;
//!
//!     // Générer immédiatement le DIDL-Lite pour MediaServer
//!     let didl = cache.get_didl(&pk, "http://localhost:8080").await?;
//!     println!("DIDL-Lite disponible immédiatement:\n{}", didl);
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
//!     cache.add_from_url("http://example.com/track1.flac", None).await?;
//!     cache.add_from_url("http://example.com/track2.flac", None).await?;
//!
//!     // Lister les collections disponibles
//!     let collections = cache.list_collections().await?;
//!     for (collection, count) in collections {
//!         println!("Collection: {} ({} pistes)", collection, count);
//!     }
//!
//!     // Récupérer toutes les pistes d'un album
//!     let tracks = cache.get_collection("pink_floyd:wish_you_were_here").await?;
//!     for entry in tracks {
//!         println!("{:02}. {} - {}",
//!             entry.metadata.track_number.unwrap_or(0),
//!             entry.metadata.title.as_deref().unwrap_or("Unknown"),
//!             entry.pk
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
//! ### Routes directes
//! - `GET /audio/tracks/{pk}/stream` - Stream le fichier FLAC (attend la conversion si nécessaire)
//! - `GET /audio/tracks/{pk}/metadata` - Récupère les métadonnées JSON (disponible immédiatement)
//! - `GET /audio/tracks/{pk}/didl` - Récupère le DIDL-Lite XML (disponible immédiatement)
//! - `GET /audio/tracks/{pk}/status` - Récupère le statut de conversion
//! - `GET /audio/stats` - Statistiques du cache
//! - `GET /audio/collections` - Liste les collections disponibles
//!
//! ### API REST (sous `/api/audio`)
//! - `GET /api/audio` - Liste toutes les pistes
//! - `POST /api/audio` - Ajoute une piste depuis une URL
//! - `GET /api/audio/{pk}` - Informations complètes d'une piste
//! - `DELETE /api/audio/{pk}` - Supprime une piste
//! - `GET /api/audio/{pk}/metadata` - Métadonnées d'une piste
//! - `GET /api/audio/{pk}/didl` - DIDL-Lite d'une piste
//! - `POST /api/audio/consolidate` - Consolide le cache (nettoie les entrées orphelines)
//! - `DELETE /api/audio` - Purge tout le cache
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
