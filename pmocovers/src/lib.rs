//! # pmocovers - Service de cache d'images de couvertures pour PMOMusic
//!
//! Cette crate fournit un système de cache d'images optimisé pour les couvertures d'albums,
//! avec conversion automatique en WebP et génération de variantes de tailles.
//!
//! ## Fonctionnalités
//!
//! - Conversion automatique en WebP pour réduire la taille
//! - Génération de variantes de tailles à la demande
//! - Cache persistant avec base de données SQLite
//! - API HTTP complète (fournie par `pmocache`)
//!
//! ## Architecture
//!
//! `pmocovers` est une spécialisation minimale de `pmocache` qui ajoute :
//! 1. La conversion WebP automatique lors du téléchargement (via transformer)
//! 2. La génération de variantes redimensionnées à la demande (via param generator)
//!
//! Tout le reste (API REST, serveur de fichiers, DB) est fourni par `pmocache`.
//!
//! ## Utilisation
//!
//! ### Exemple minimal
//!
//! ```rust,no_run
//! use pmocovers::cache;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let cache = cache::new_cache("./covers_cache", 200)?;
//!     let pk = cache.add_from_url("https://example.com/cover.jpg", None).await?;
//!     let path = cache.get(&pk).await?;
//!     println!("Image convertie en WebP: {path:?}");
//!     Ok(())
//! }
//! ```
//!
//! ### Exemple avec configuration automatique
//!
//! ```rust,no_run
//! use pmocovers::CoverCacheExt;
//! use pmoserver::ServerBuilder;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut server = ServerBuilder::new_configured().build();
//!     server.init_cover_cache_configured().await?;
//!     server.start().await;
//!     server.wait().await;
//!     Ok(())
//! }
//! ```

pub mod cache;
pub mod webp;


#[cfg(feature = "pmoserver")]
pub mod openapi;

#[cfg(feature = "pmoconfig")]
pub mod config_ext;

pub use cache::{new_cache, new_cache_with_consolidation, Cache, CoversConfig};

#[cfg(feature = "pmoserver")]
pub use openapi::ApiDoc;

#[cfg(feature = "pmoconfig")]
pub use config_ext::CoverCacheConfigExt;

// ============================================================================
// Registre global singleton
// ============================================================================

use once_cell::sync::OnceCell;
use std::sync::Arc;

static COVER_CACHE: OnceCell<Arc<Cache>> = OnceCell::new();

/// Enregistre le cache de couvertures global
///
/// Cette fonction doit être appelée au démarrage de l'application
/// pour rendre le cache de couvertures disponible globalement.
///
/// # Arguments
///
/// * `cache` - Instance partagée du cache de couvertures à enregistrer
///
/// # Behavior
///
/// - Si appelée plusieurs fois, seul le premier appel prend effet
/// - Thread-safe: peut être appelée depuis plusieurs threads simultanément
/// - Une fois enregistré, le cache est accessible via [`get_cover_cache`]
///
/// # Examples
///
/// ```rust,ignore
/// use pmocovers::{new_cache, register_cover_cache};
/// use std::sync::Arc;
///
/// let cache = Arc::new(new_cache("./covers", 100)?);
/// register_cover_cache(cache);
/// ```
pub fn register_cover_cache(cache: Arc<Cache>) {
    let _ = COVER_CACHE.set(cache);
}

/// Accès global au cache de couvertures
///
/// Retourne une référence au cache de couvertures enregistré via [`register_cover_cache`],
/// ou `None` si aucun cache n'a été enregistré.
///
/// # Returns
///
/// * `Some(Arc<Cache>)` - Instance partagée du cache de couvertures si enregistré
/// * `None` - Si aucun cache n'a été enregistré
///
/// # Thread Safety
///
/// Cette fonction est thread-safe et peut être appelée depuis plusieurs threads.
///
/// # Examples
///
/// ```rust,ignore
/// use pmocovers::get_cover_cache;
///
/// if let Some(cache) = get_cover_cache() {
///     // Utiliser le cache
/// }
/// ```
pub fn get_cover_cache() -> Option<Arc<Cache>> {
    COVER_CACHE.get().cloned()
}

// ============================================================================
// Extension pmoserver
// ============================================================================

#[cfg(feature = "pmoserver")]
use utoipa::OpenApi;

/// Générateur de variantes d'images
///
/// Si param est numérique, génère une variante redimensionnée
#[cfg(feature = "pmoserver")]
fn create_variant_generator() -> pmocache::pmoserver_ext::ParamGenerator<CoversConfig> {
    Arc::new(|cache, pk, param| {
        Box::pin(async move {
            // Si le param est numérique, c'est une taille de variante
            if let Ok(size) = param.parse::<usize>() {
                match webp::generate_variant(&cache, &pk, size).await {
                    Ok(data) => return Some(data),
                    Err(e) => {
                        tracing::warn!(
                            "Cannot generate variant {}x{} for {}: {}",
                            size,
                            size,
                            pk,
                            e
                        );
                        return None;
                    }
                }
            }
            // Param non reconnu
            None
        })
    })
}

/// Trait d'extension pour ajouter le cache de couvertures à pmoserver
#[cfg(feature = "pmoserver")]
#[async_trait::async_trait]
pub trait CoverCacheExt {
    /// Initialise le cache d'images et enregistre les routes HTTP
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Répertoire de stockage du cache
    /// * `limit` - Limite de taille du cache (en nombre d'images)
    ///
    /// # Returns
    ///
    /// Instance partagée du cache
    ///
    /// # Routes enregistrées
    ///
    /// - `GET /covers/image/{pk}` - Image originale
    /// - `GET /covers/image/{pk}/{size}` - Variante de taille (ex: 256, 512)
    /// - `GET /api/covers` - Liste des images (API REST)
    /// - `POST /api/covers` - Ajouter une image (API REST)
    /// - `DELETE /api/covers/{pk}` - Supprimer une image (API REST)
    /// - `GET /api/covers/{pk}/status` - Statut du téléchargement
    /// - `GET /swagger-ui/covers` - Documentation interactive
    async fn init_cover_cache(
        &mut self,
        cache_dir: &str,
        limit: usize,
    ) -> anyhow::Result<Arc<Cache>>;

    /// Initialise le cache d'images avec la configuration par défaut
    ///
    /// Utilise automatiquement les paramètres de `pmoconfig::Config`
    async fn init_cover_cache_configured(&mut self) -> anyhow::Result<Arc<Cache>>;
}

#[cfg(feature = "pmoserver")]
#[async_trait::async_trait]
impl CoverCacheExt for pmoserver::Server {
    async fn init_cover_cache(
        &mut self,
        cache_dir: &str,
        limit: usize,
    ) -> anyhow::Result<Arc<Cache>> {
        use pmocache::pmoserver_ext::{create_api_router, create_file_router_with_generator};

        let cache = Arc::new(cache::new_cache(cache_dir, limit)?);

        // Router de fichiers avec génération de variantes
        // Routes: GET /covers/image/{pk} et GET /covers/image/{pk}/{size}
        let file_router = create_file_router_with_generator(
            cache.clone(),
            "image/webp",
            Some(create_variant_generator()),
        );
        self.add_router("/", file_router).await;

        // API REST générique (pmocache)
        // Routes: GET/POST/DELETE /api/covers, etc.
        let api_router = create_api_router(cache.clone());
        let openapi = crate::ApiDoc::openapi();
        self.add_openapi(api_router, openapi, "covers").await;

        Ok(cache)
    }

    async fn init_cover_cache_configured(&mut self) -> anyhow::Result<Arc<Cache>> {
        use crate::CoverCacheConfigExt;
        let config = pmoconfig::get_config();
        let cache_dir = config.get_covers_dir()?;
        let limit = config.get_covers_size()?;
        self.init_cover_cache(&cache_dir, limit).await
    }
}
