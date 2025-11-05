//! # pmoaudiocache – Cache de pistes audio pour PMOMusic
//!
//! `pmoaudiocache` s'appuie sur [`pmocache`] pour fournir un cache spécialisé
//! dans les fichiers audio. Il assure la conversion transparente au format FLAC,
//! l'extraction des métadonnées et la mise à disposition d'outils pour les exposer.
//!
//! ## Fonctionnalités
//!
//! - conversion automatique des entrées en FLAC grâce à un `StreamTransformer` ;
//! - extraction des tags (artiste, album, titre, etc.) via [`metadata::AudioMetadata`] ;
//! - stockage des métadonnées dans la table `metadata` de `pmocache::DB` ;
//! - helpers pour renseigner les collections à partir des tags ;
//! - intégration optionnelle avec `pmoserver` (routes REST + diffusion de fichiers).
//!
//! ## Exemple rapide
//!
//! ```rust,no_run
//! use pmoaudiocache::cache;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let cache = cache::new_cache("./audio_cache", 500)?;
//!
//!     // Télécharge la piste, déclenche la conversion FLAC et stocke les métadonnées.
//!     let pk = cache::add_with_metadata_extraction(
//!         &cache,
//!         "https://example.com/track.mp3",
//!         None,
//!     ).await?;
//!
//!     // Lecture des métadonnées extraites
//!     let metadata = cache::get_metadata(&cache, &pk)?;
//!     println!(
//!         "Titre: {}",
//!         metadata.title.as_deref().unwrap_or("Inconnu")
//!     );
//!
//!     // Accès au fichier FLAC converti
//!     let flac_path = cache.get(&pk).await?;
//!     println!("Fichier converti: {flac_path:?}");
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Intégration serveur (feature `pmoserver`)
//!
//! Lorsque la feature `pmoserver` est activée, [`AudioCacheExt`] permet
//! d'enregistrer automatiquement les routes suivantes :
//!
//! - `GET /audio/tracks/{pk}` : téléchargement/stream du FLAC original ;
//! - `GET /audio/tracks/{pk}/{qualifier}` : variantes (ex: `orig`) ;
//! - `GET /api/audio` / `POST /api/audio` / `DELETE /api/audio` : API REST générique ;
//! - `GET /api/audio/{pk}/status` : suivi de téléchargement ;
//! - endpoints OpenAPI/Swagger lorsqu'`openapi` est activée.
//!
//! ## Métadonnées gérées
//!
//! Le module [`metadata`] extrait notamment :
//! - titre, artiste, album, genre ;
//! - numéros de piste/disque et totaux associés ;
//! - année, durée, bitrate, sample rate, nombre de canaux.
//!
//! En l'absence d'artiste/album, aucune collection automatique n'est créée.
//!
//! ## Modules
//!
//! - [`cache`] : instanciation du cache et helpers de téléchargement ;
//! - [`metadata`] : extraction/structure des métadonnées audio ;
//! - [`config_ext`] *(feature `pmoconfig`)* : dérivation de la configuration depuis `pmoconfig`;
//! - [`openapi`] *(feature `pmoserver`)* : documentation des routes REST.
//!
//! ## Crates voisines
//!
//! - [`pmocache`] : fondation générique ;
//! - [`pmocovers`] : spécialisation images (architecture similaire) ;
//! - [`pmoserver`] : serveur HTTP optionnel.

pub mod cache;
pub mod metadata;
pub mod metadata_ext;
pub mod streaming;
pub mod track_metadata;

#[cfg(feature = "pmoserver")]
pub mod openapi;

#[cfg(feature = "pmoconfig")]
pub mod config_ext;

// Re-exports principaux
pub use cache::{add_with_metadata_extraction, get_metadata, new_cache, AudioConfig, Cache};
pub use metadata::AudioMetadata;
pub use metadata_ext::{AudioMetadataExt, AudioTrackMetadataExt};
pub use track_metadata::AudioCacheTrackMetadata;

#[cfg(feature = "pmoconfig")]
pub use config_ext::AudioCacheConfigExt;

#[cfg(feature = "pmoserver")]
pub use openapi::ApiDoc;

// ============================================================================
// Registre global singleton
// ============================================================================

use once_cell::sync::OnceCell;
use std::sync::Arc;

static AUDIO_CACHE: OnceCell<Arc<Cache>> = OnceCell::new();

/// Enregistre le cache audio global
///
/// Cette fonction doit être appelée au démarrage de l'application
/// pour rendre le cache audio disponible globalement.
///
/// # Examples
///
/// ```rust,ignore
/// use pmoaudiocache::{new_cache, register_audio_cache};
/// use std::sync::Arc;
///
/// let cache = Arc::new(new_cache("./cache", 1000)?);
/// register_audio_cache(cache);
/// ```
pub fn register_audio_cache(cache: Arc<Cache>) {
    let _ = AUDIO_CACHE.set(cache);
}

/// Accès global au cache audio
///
/// # Examples
///
/// ```rust,ignore
/// use pmoaudiocache::get_audio_cache;
///
/// if let Some(cache) = get_audio_cache() {
///     // Utiliser le cache
/// }
/// ```
pub fn get_audio_cache() -> Option<Arc<Cache>> {
    AUDIO_CACHE.get().cloned()
}

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
