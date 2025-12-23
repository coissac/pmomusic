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
pub mod api;

#[cfg(feature = "pmoserver")]
pub mod openapi;

#[cfg(feature = "pmoconfig")]
pub mod config_ext;

pub use cache::{add_local_file, new_cache, new_cache_with_consolidation, Cache, CoversConfig};

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

// ========================================================================
// Handlers JPEG (transcodage à la volée)
// ========================================================================

/// Sert une image de couverture au format JPEG (transcodage depuis WebP)
///
/// Cette route transcode à la volée l'image WebP stockée en cache vers le format JPEG.
/// Utile pour la compatibilité avec les clients qui ne supportent pas WebP (ex: UPnP).
///
/// # Arguments
///
/// * `pk` - Clé primaire de l'image
///
/// # Responses
///
/// * `200 OK` - Image JPEG transcodée
/// * `404 NOT_FOUND` - Image non trouvée
/// * `500 INTERNAL_SERVER_ERROR` - Erreur de transcodage
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    get,
    path = "/covers/jpeg/{pk}",
    tag = "covers",
    params(
        ("pk" = String, Path, description = "Clé primaire de l'image")
    ),
    responses(
        (status = 200, description = "Image JPEG", content_type = "image/jpeg"),
        (status = 404, description = "Image non trouvée"),
        (status = 500, description = "Erreur de transcodage"),
    )
)]
async fn serve_cover_jpeg(
    axum::extract::State(cache): axum::extract::State<Arc<Cache>>,
    axum::extract::Path(pk): axum::extract::Path<String>,
) -> axum::response::Response {
    serve_jpeg_internal(cache, pk, None).await
}

/// Sert une image de couverture redimensionnée au format JPEG (transcodage depuis WebP)
///
/// Cette route transcode à la volée l'image WebP stockée en cache vers le format JPEG,
/// en la redimensionnant à la taille demandée (format carré).
///
/// # Arguments
///
/// * `pk` - Clé primaire de l'image
/// * `size` - Taille souhaitée en pixels (ex: 256 pour 256x256)
///
/// # Responses
///
/// * `200 OK` - Image JPEG redimensionnée et transcodée
/// * `404 NOT_FOUND` - Image non trouvée
/// * `500 INTERNAL_SERVER_ERROR` - Erreur de transcodage ou redimensionnement
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    get,
    path = "/covers/jpeg/{pk}/{size}",
    tag = "covers",
    params(
        ("pk" = String, Path, description = "Clé primaire de l'image"),
        ("size" = String, Path, description = "Taille en pixels (ex: 256, 512)")
    ),
    responses(
        (status = 200, description = "Image JPEG redimensionnée", content_type = "image/jpeg"),
        (status = 404, description = "Image non trouvée"),
        (status = 500, description = "Erreur de transcodage"),
    )
)]
async fn serve_cover_jpeg_with_size(
    axum::extract::State(cache): axum::extract::State<Arc<Cache>>,
    axum::extract::Path((pk, size)): axum::extract::Path<(String, String)>,
) -> axum::response::Response {
    let size = size.parse::<u32>().ok();
    serve_jpeg_internal(cache, pk, size).await
}

#[cfg(feature = "pmoserver")]
async fn serve_jpeg_internal(
    cache: Arc<Cache>,
    pk: String,
    size: Option<u32>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use image::ImageFormat;
    use std::io::Cursor;

    let path = cache.get_file_path_with_qualifier(
        &pk,
        <CoversConfig as pmocache::cache::CacheConfig>::default_param(),
    );
    if !path.exists() {
        return (StatusCode::NOT_FOUND, "File not found").into_response();
    }

    let res = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let mut img = image::open(&path)?;
        if let Some(size) = size {
            img = crate::webp::ensure_square(&img, size);
        }
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Jpeg)?;
        Ok(buf.into_inner())
    })
    .await;

    match res {
        Ok(Ok(data)) => (StatusCode::OK, [("content-type", "image/jpeg")], data).into_response(),
        Ok(Err(e)) => {
            tracing::warn!("JPEG transcode error for {}: {}", pk, e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Transcode error").into_response()
        }
        Err(e) => {
            tracing::warn!("JPEG transcode join error for {}: {}", pk, e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Transcode error").into_response()
        }
    }
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
        use pmocache::pmoserver_ext::create_file_router_with_generator;

        let cache = Arc::new(cache::new_cache(cache_dir, limit)?);

        // Router de fichiers avec génération de variantes
        // Routes: GET /covers/image/{pk} et GET /covers/image/{pk}/{size}
        let file_router = create_file_router_with_generator(
            cache.clone(),
            "image/webp",
            Some(create_variant_generator()),
        );

        // Router JPEG (transcodage à la volée depuis le WebP stocké)
        // Routes: GET /covers/jpeg/{pk} et GET /covers/jpeg/{pk}/{size}
        let jpeg_router = axum::Router::new()
            .route("/covers/jpeg/{pk}", axum::routing::get(serve_cover_jpeg))
            .route(
                "/covers/jpeg/{pk}/{size}",
                axum::routing::get(serve_cover_jpeg_with_size),
            )
            .with_state(cache.clone());

        // Combiner WebP et JPEG dans un seul sous-router pour éviter tout overlap
        let combined_router = file_router.merge(jpeg_router);
        self.add_router("/", combined_router).await;

        // API REST (handlers génériques + POST spécialisé covers)
        let api_router = axum::Router::new()
            .route(
                "/",
                axum::routing::get(pmocache::api::list_items::<CoversConfig>)
                    .post(crate::api::add_cover_item)
                    .delete(pmocache::api::purge_cache::<CoversConfig>),
            )
            .route(
                "/{pk}",
                axum::routing::get(pmocache::api::get_item_info::<CoversConfig>)
                    .delete(pmocache::api::delete_item::<CoversConfig>),
            )
            .route(
                "/{pk}/status",
                axum::routing::get(pmocache::api::get_download_status::<CoversConfig>),
            )
            .route(
                "/consolidate",
                axum::routing::post(pmocache::api::consolidate_cache::<CoversConfig>),
            )
            .with_state(cache.clone());

        let openapi = crate::ApiDoc::openapi();
        self.add_openapi(api_router, openapi, "covers").await;

        // Enregistrer dans le singleton global pour éviter des initialisations multiples
        register_cover_cache(cache.clone());

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
