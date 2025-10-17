//! # pmocovers - Service de cache d'images de couvertures pour PMOMusic
//!
//! Cette crate fournit un système de cache d'images optimisé pour les couvertures d'albums,
//! avec conversion automatique en WebP et génération de variantes de tailles.
//!
//! ## Vue d'ensemble
//!
//! `pmocovers` gère le téléchargement, la conversion, le stockage et la distribution
//! d'images de couvertures d'albums, avec :
//! - Conversion automatique en WebP pour réduire la taille
//! - Génération de variantes de tailles à la demande
//! - Cache persistant avec base de données SQLite
//! - API HTTP pour récupérer les images
//!
//! ## Fonctionnalités
//!
//! ### 📦 Gestion du cache
//! - Téléchargement automatique depuis des URLs
//! - Conversion des images en WebP (format optimisé)
//! - Stockage persistant sur disque
//! - Base de données SQLite pour le tracking
//!
//! ### 🎨 Génération de variantes
//! - Redimensionnement automatique à la demande
//! - Création d'images carrées avec centrage
//! - Cache des variantes générées
//! - Support de multiples tailles
//!
//! ### 📊 Statistiques d'utilisation
//! - Comptage des accès (hits)
//! - Suivi de la dernière utilisation
//! - API de statistiques complètes
//!
//! ## Architecture
//!
//! `pmocovers` suit le pattern d'extension des autres crates PMO :
//!
//! - `pmoserver` définit un serveur HTTP générique
//! - `pmocovers` étend ce serveur avec des méthodes de cache via un trait
//! - Le serveur n'a pas besoin de connaître `pmocovers`
//!
//! ## Structure des fichiers
//!
//! ```text
//! pmocovers/
//! ├── Cargo.toml
//! ├── src/
//! │   ├── lib.rs              # Module principal (ce fichier)
//! │   ├── cache.rs            # Gestion du cache
//! │   ├── db.rs               # Base de données SQLite
//! │   ├── webp.rs             # Conversion et redimensionnement WebP
//! │   └── pmoserver_impl.rs   # Extension de pmoserver::Server
//! └── cache/                  # Répertoire de cache (généré)
//!     ├── cache.db            # Base SQLite
//!     ├── *.orig.webp         # Images originales
//!     └── *.{size}.webp       # Variantes de tailles
//! ```
//!
//! ## Utilisation
//!
//! ### Exemple basique avec configuration automatique
//!
//! ```rust,no_run
//! use pmocovers::CoverCacheExt;
//! use pmoserver::ServerBuilder;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut server = ServerBuilder::new_configured().build();
//!
//!     // Utilise automatiquement la config (pmoconfig)
//!     server.init_cover_cache_configured().await?;
//!
//!     server.start().await;
//!     server.wait().await;
//!     Ok(())
//! }
//! ```
//!
//! ### Exemple avec paramètres personnalisés
//!
//! ```rust,no_run
//! use pmocovers::CoverCacheExt;
//! use pmoserver::ServerBuilder;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut server = ServerBuilder::new("MyApp", "http://localhost:3000", 3000).build();
//!
//!     // Paramètres personnalisés
//!     server.init_cover_cache("./cache", 1000).await?;
//!
//!     server.start().await;
//!     server.wait().await;
//!     Ok(())
//! }
//! ```
//!
//! ### Utilisation du cache directement
//!
//! ```rust,no_run
//! use pmocovers::Cache;
//! use pmocache::FileCache;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let cache = Cache::new("./cache", 1000, "http://localhost:8080")?;
//!
//!     // Ajouter une image depuis une URL (avec conversion WebP automatique)
//!     let pk = cache.add_from_url("http://example.com/cover.jpg", None).await?;
//!     println!("Image ajoutée avec clé: {}", pk);
//!
//!     // Récupérer l'image originale
//!     let path = cache.get(&pk).await?;
//!     println!("Image stockée à: {:?}", path);
//!
//!     Ok(())
//! }
//! ```
//!
//! ## API HTTP
//!
//! Une fois enregistré sur un serveur via `CoverCacheExt`, les endpoints suivants sont disponibles :
//!
//! ### GET /covers/images/{pk}
//! Récupère l'image originale en WebP
//!
//! ### GET /covers/images/{pk}/{size}
//! Récupère une variante de taille spécifique (ex: `/covers/images/abc123/256`)
//!
//! ### GET /covers/stats
//! Récupère les statistiques du cache (JSON)
//!
//! ## Format des clés (pk)
//!
//! Les images sont identifiées par une clé (pk) dérivée de l'URL source :
//! - Hash SHA1 de l'URL
//! - Encodé en hexadécimal (8 premiers octets)
//! - Exemple: `"1a2b3c4d5e6f7a8b"`
//!
//! ## Stockage
//!
//! Les fichiers sont organisés comme suit :
//!
//! ```text
//! cache/
//! ├── cache.db                      # Base SQLite
//! ├── 1a2b3c4d.orig.webp            # Image originale
//! ├── 1a2b3c4d.256.webp             # Variante 256x256
//! └── 1a2b3c4d.512.webp             # Variante 512x512
//! ```
//!
//! ## Opérations de maintenance
//!
//! ### Purge du cache
//!
//! ```rust,no_run
//! # use pmocovers::Cache;
//! # async fn example(cache: &Cache) -> anyhow::Result<()> {
//! // Supprimer tous les fichiers et entrées DB
//! cache.purge().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Consolidation du cache
//!
//! ```rust,no_run
//! # use pmocovers::Cache;
//! # async fn example(cache: &Cache) -> anyhow::Result<()> {
//! // Re-télécharger les images manquantes et supprimer les orphelins
//! cache.consolidate().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Dépendances principales
//!
//! - `image` : Chargement et manipulation d'images
//! - `webp` : Encodage WebP
//! - `rusqlite` : Base de données SQLite
//! - `reqwest` : Téléchargement HTTP
//! - `sha1` : Génération de clés
//!
//! ## Voir aussi
//!
//! - [`pmoserver`] : Serveur HTTP Axum
//! - [`pmoapp`] : Application web frontend
//! - [`pmoupnp`] : Bibliothèque UPnP MediaRenderer

pub mod cache;
pub mod db;
pub mod webp;

#[cfg(feature = "pmoserver")]
pub mod api;

#[cfg(feature = "pmoserver")]
pub mod openapi;

pub use cache::{Cache, CoversConfig};
pub use db::{CacheEntry, DB};

#[cfg(feature = "pmoserver")]
pub use openapi::ApiDoc;

use anyhow::Result;
use std::sync::Arc;

/// Trait pour étendre un serveur HTTP avec des fonctionnalités de cache d'images.
///
/// Ce trait permet à `pmocovers` d'ajouter des méthodes d'extension sur des types
/// de serveurs externes (comme `pmoserver::Server`) sans que ces crates dépendent de `pmocovers`.
///
/// # Architecture
///
/// Similaire au pattern utilisé par `pmoapp` pour `WebAppExt`, ce trait permet
/// une extension propre et découplée :
///
/// - `pmoserver` définit un serveur HTTP générique
/// - `pmocovers` étend ce serveur avec des méthodes de cache via ce trait
/// - Le serveur n'a pas besoin de connaître `pmocovers`
pub trait CoverCacheExt {
    /// Initialise le cache d'images et enregistre les routes HTTP.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Répertoire de stockage du cache
    /// * `limit` - Limite de taille du cache (en nombre d'images)
    ///
    /// # Returns
    ///
    /// * `Arc<Cache>` - Instance partagée du cache
    ///
    /// # Routes enregistrées
    ///
    /// - `GET /covers/images/{pk}` - Image originale
    /// - `GET /covers/images/{pk}/{size}` - Variante de taille
    /// - `GET /covers/stats` - Statistiques
    /// - `GET /api/covers` - Liste des images (API REST)
    /// - `POST /api/covers` - Ajouter une image (API REST)
    /// - `DELETE /api/covers/{pk}` - Supprimer une image (API REST)
    /// - `GET /swagger-ui` - Documentation interactive
    async fn init_cover_cache(&mut self, cache_dir: &str, limit: usize) -> Result<Arc<Cache>>;

    /// Initialise le cache d'images avec la configuration par défaut.
    ///
    /// Utilise automatiquement les paramètres de `pmoconfig::Config` :
    /// - `host.cover_cache.directory` pour le répertoire
    /// - `host.cover_cache.size` pour la limite de taille
    ///
    /// # Returns
    ///
    /// * `Arc<Cache>` - Instance partagée du cache
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmocovers::CoverCacheExt;
    /// use pmoserver::ServerBuilder;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let mut server = ServerBuilder::new_configured().build();
    ///
    ///     // Utilise automatiquement la config
    ///     server.init_cover_cache_configured().await?;
    ///
    ///     server.start().await;
    ///     Ok(())
    /// }
    /// ```
    async fn init_cover_cache_configured(&mut self) -> Result<Arc<Cache>>;
}

// Implémentation du trait pour pmoserver::Server (feature-gated)
#[cfg(feature = "pmoserver")]
mod pmoserver_impl;
