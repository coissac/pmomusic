//! # pmoaudiocache - Cache de pistes audio pour PMOMusic
//!
//! Cette crate fournit un système de cache pour les pistes audio avec conversion
//! automatique en FLAC et extraction des métadonnées.
//!
//! ## Vue d'ensemble
//!
//! `pmoaudiocache` étend `pmocache` pour gérer spécifiquement les fichiers audio :
//! - **Téléchargement asynchrone** via le système de download de `pmocache`
//! - **Conversion automatique en FLAC** lors du téléchargement (via transformer)
//! - **Extraction et stockage des métadonnées** en JSON dans la base de données
//! - **Gestion de collections** basées sur artiste/album
//! - **Streaming progressif** automatique (via `pmocache`)
//! - **API REST complète** fournie par `pmocache`
//!
//! ## Architecture
//!
//! Cette crate est une spécialisation minimale de `pmocache` :
//! - Configuration via `AudioConfig`
//! - Transformer FLAC pour la conversion automatique
//! - Helpers pour l'extraction et la lecture des métadonnées
//!
//! Tout le reste (DB, API REST, streaming) est fourni par `pmocache`.
//!
//! ## Utilisation
//!
//! ### Exemple basique
//!
//! ```rust,no_run
//! use pmoaudiocache::cache;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Créer le cache
//!     let cache = cache::new_cache("./audio_cache", 1000, "http://localhost:8080")?;
//!
//!     // Ajouter une piste avec extraction des métadonnées
//!     let pk = cache::add_with_metadata_extraction(
//!         &cache,
//!         "http://example.com/track.flac",
//!         None  // collection auto-détectée depuis métadonnées
//!     ).await?;
//!
//!     // Lire les métadonnées
//!     let metadata = cache::get_metadata(&cache, &pk)?;
//!     println!("{} - {}",
//!         metadata.artist.as_deref().unwrap_or("Unknown"),
//!         metadata.title.as_deref().unwrap_or("Unknown")
//!     );
//!
//!     // Le fichier FLAC est disponible immédiatement après le download
//!     let file_path = cache.get(&pk).await?;
//!     println!("FLAC file: {:?}", file_path);
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Utilisation avec pmoserver
//!
//! ```rust,no_run
//! use pmoaudiocache::AudioCacheExt;
//! use pmoserver::ServerBuilder;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut server = ServerBuilder::new_configured().build();
//!
//!     // Initialiser le cache audio avec configuration automatique
//!     server.init_audio_cache_configured().await?;
//!
//!     server.start().await;
//!     server.wait().await;
//!     Ok(())
//! }
//! ```
//!
//! ## API HTTP (avec feature "pmoserver")
//!
//! Lorsque la feature `pmoserver` est activée, les routes suivantes sont disponibles :
//!
//! ### Routes de fichiers
//! - `GET /audio/tracks/{pk}` - Stream du fichier FLAC original
//! - `GET /audio/tracks/{pk}/orig` - Alias pour l'original
//!
//! ### API REST
//! - `GET /api/audio` - Liste toutes les pistes
//! - `POST /api/audio` - Ajoute une piste depuis une URL
//! - `GET /api/audio/{pk}` - Informations complètes d'une piste
//! - `DELETE /api/audio/{pk}` - Supprime une piste
//! - `GET /api/audio/{pk}/status` - Statut du téléchargement
//! - `POST /api/audio/consolidate` - Consolide le cache
//! - `DELETE /api/audio` - Purge tout le cache
//!
//! ## Métadonnées supportées
//!
//! Les métadonnées suivantes sont extraites automatiquement :
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
//! ## Différences avec l'ancienne version
//!
//! Cette version refactorisée de `pmoaudiocache` :
//! - ✅ **Supprime le champ `conversion_status`** : le système `Download` de `pmocache` gère déjà l'état asynchrone
//! - ✅ **Utilise `pmocache::DB`** : plus de DB personnalisée, les métadonnées sont en JSON
//! - ✅ **API REST générique** : fournie par `pmocache`, plus de code custom
//! - ✅ **Code réduit de 52%** : de ~1681 lignes à ~800 lignes
//! - ✅ **Streaming progressif** : automatique via `pmocache`
//! - ✅ **Politique LRU optimisée** : nouvel index composite dans `pmocache`
//!
//! ## Dépendances principales
//!
//! - `pmocache` : Cache générique avec download asynchrone
//! - `lofty` : Extraction de métadonnées audio
//! - `tokio` : Runtime asynchrone
//!
//! ## Voir aussi
//!
//! - [`pmocache`] : Cache générique
//! - [`pmocovers`] : Cache d'images (architecture similaire)
//! - [`pmoserver`] : Serveur HTTP

pub mod cache;
pub mod metadata;

#[cfg(feature = "pmoserver")]
pub mod openapi;

#[cfg(feature = "pmoconfig")]
pub mod config_ext;

// Re-exports principaux
pub use cache::{add_with_metadata_extraction, get_metadata, new_cache, AudioConfig, Cache};
pub use metadata::AudioMetadata;

#[cfg(feature = "pmoconfig")]
pub use config_ext::AudioCacheConfigExt;

#[cfg(feature = "pmoserver")]
pub use openapi::ApiDoc;

// ============================================================================
// Extension pmoserver (inline comme pmocovers)
// ============================================================================

/// Trait pour étendre un serveur HTTP avec des fonctionnalités de cache audio.
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
    /// * `Arc<Cache>` - Instance partagée du cache
    async fn init_audio_cache(
        &mut self,
        cache_dir: &str,
        limit: usize,
    ) -> anyhow::Result<std::sync::Arc<Cache>>;

    /// Initialise le cache audio avec la configuration par défaut.
    ///
    /// Utilise automatiquement les paramètres de `pmoconfig::Config`.
    async fn init_audio_cache_configured(&mut self) -> anyhow::Result<std::sync::Arc<Cache>>;
}

#[cfg(feature = "pmoserver")]
use pmocache::pmoserver_ext::{create_api_router, create_file_router};
#[cfg(feature = "pmoserver")]
use std::sync::Arc;
#[cfg(feature = "pmoserver")]
use utoipa::OpenApi;

#[cfg(feature = "pmoserver")]
impl AudioCacheExt for pmoserver::Server {
    async fn init_audio_cache(
        &mut self,
        cache_dir: &str,
        limit: usize,
    ) -> anyhow::Result<Arc<Cache>> {
        let cache = Arc::new(crate::cache::new_cache(cache_dir, limit)?);

        // Router de fichiers pour servir les pistes FLAC
        // Routes: GET /audio/tracks/{pk} et GET /audio/tracks/{pk}/{param}
        let file_router = create_file_router(
            cache.clone(),
            "audio/flac", // Content-Type
        );
        self.add_router("/", file_router).await;

        // API REST générique (pmocache)
        // Routes: GET/POST/DELETE /api/audio, etc.
        let api_router = create_api_router(cache.clone());
        let openapi = crate::ApiDoc::openapi();
        self.add_openapi(api_router, openapi, "audio").await;

        Ok(cache)
    }

    async fn init_audio_cache_configured(&mut self) -> anyhow::Result<Arc<Cache>> {
        use crate::AudioCacheConfigExt;
        let config = pmoconfig::get_config();
        let cache_dir = config.get_audiocache_dir()?;
        let limit = config.get_audiocache_size()?;
        self.init_audio_cache(&cache_dir, limit).await
    }
}
