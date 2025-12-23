//! Extension pmoserver pour servir les fichiers du cache via HTTP
//!
//! Ce module fournit des handlers génériques pour servir les fichiers
//! d'un cache via des routes HTTP structurées, avec support du streaming progressif.
//!
//! ## Routes générées
//!
//! Format: `/{cache_name}/{cache_type}/{pk}[/{param}]`
//!
//! Exemples:
//! - `/covers/images/abc123` - Image avec param par défaut (orig)
//! - `/covers/images/abc123/256` - Image redimensionnée 256x256
//! - `/audio/tracks/def456` - Piste audio par défaut
//! - `/audio/tracks/def456/stream` - Piste audio streamable
//!
//! ## Streaming progressif
//!
//! Les fichiers en cours de téléchargement sont automatiquement streamés
//! au fur et à mesure de leur disponibilité.
//!
//! ## Utilisation
//!
//! ```rust,no_run
//! use pmocache::pmoserver_ext;
//! use axum::Router;
//!
//! # async fn example(cache: std::sync::Arc<pmocache::Cache<CoversConfig>>) {
//! // Créer un router pour servir les fichiers
//! let router = pmoserver_ext::create_file_router(
//!     cache.clone(),
//!     "image/webp"  // Content-Type
//! );
//!
//! // Le router sera monté à la racine avec les routes complètes
//! // Exemple: GET /covers/images/{pk}
//! //          GET /covers/images/{pk}/{param}
//! # }
//! ```

#[cfg(feature = "pmoserver")]
use crate::{Cache, CacheConfig};
#[cfg(feature = "pmoserver")]
use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
#[cfg(feature = "pmoserver")]
use std::future::Future;
#[cfg(feature = "pmoserver")]
use std::pin::Pin;
#[cfg(feature = "pmoserver")]
use std::sync::Arc;
#[cfg(feature = "pmoserver")]
use tokio_util::io::ReaderStream;
#[cfg(feature = "pmoserver")]
use tracing::warn;

/// Type pour le callback de génération de param
///
/// Appelé quand un fichier avec param n'existe pas.
/// Permet de générer à la volée (ex: redimensionnement d'images).
///
/// # Arguments
///
/// - `cache`: le cache
/// - `pk`: clé primaire
/// - `param`: paramètre demandé (ex: "256" pour une taille)
///
/// # Retourne
///
/// Les données générées ou None si le param n'est pas supporté
#[cfg(feature = "pmoserver")]
pub type ParamGenerator<C> = Arc<
    dyn Fn(Arc<Cache<C>>, String, String) -> Pin<Box<dyn Future<Output = Option<Vec<u8>>> + Send>>
        + Send
        + Sync,
>;

/// Handler générique pour GET /{cache_name}/{cache_type}/{pk}
/// Sert un fichier avec le param par défaut
#[cfg(feature = "pmoserver")]
async fn get_file<C: CacheConfig + 'static>(
    State((cache, content_type, param_generator)): State<(
        Arc<Cache<C>>,
        &'static str,
        Option<ParamGenerator<C>>,
    )>,
    Path(pk): Path<String>,
) -> Response {
    // Utiliser le param par défaut
    let param = C::default_param();
    serve_file_with_streaming(&cache, &pk, param, content_type, param_generator).await
}

/// Handler générique pour GET /{cache_name}/{cache_type}/{pk}/{param}
/// Sert un fichier avec un param spécifique
#[cfg(feature = "pmoserver")]
async fn get_file_with_param<C: CacheConfig + 'static>(
    State((cache, content_type, param_generator)): State<(
        Arc<Cache<C>>,
        &'static str,
        Option<ParamGenerator<C>>,
    )>,
    Path((pk, param)): Path<(String, String)>,
) -> Response {
    serve_file_with_streaming(&cache, &pk, &param, content_type, param_generator).await
}

#[cfg(feature = "pmoserver")]
async fn serve_finalized_pk<C: CacheConfig + 'static>(
    cache: &Arc<Cache<C>>,
    pk: &str,
    param: &str,
    content_type: &'static str,
) -> Response {
    let qualifier = param.to_string();
    let file_path = cache.get_file_path_with_qualifier(pk, param);

    if let Err(e) = cache.db.update_hit(pk) {
        warn!("Error updating hit count for {}: {}", pk, e);
    }

    let response = serve_complete_file(file_path, content_type).await;

    if response.status().is_success() {
        cache.notify_broadcast(pk, &qualifier).await;
    }

    response
}

/// Handler spécifique pour les lazy PK
///
/// Gère le téléchargement on-demand des fichiers lazy :
/// 1. Fast path : vérifie si déjà téléchargé
/// 2. Résout l'URL via la DB ou un provider
/// 3. Lance le téléchargement et calcule le real pk
/// 4. Met à jour la DB (lazy → downloaded)
/// 5. Broadcast l'event pour PK switching
/// 6. Sert directement le fichier téléchargé
#[cfg(feature = "pmoserver")]
async fn serve_lazy_audio_file<C: CacheConfig + 'static>(
    cache: &Arc<Cache<C>>,
    lazy_pk: &str,
    param: &str,
    content_type: &'static str,
) -> Response {
    tracing::info!("Lazy download triggered for pk: {}", lazy_pk);

    // 1. Vérifier si déjà téléchargé (fast path)
    if let Ok(Some(real_pk)) = cache.db.get_pk_by_lazy_pk(lazy_pk) {
        tracing::debug!(
            "Lazy PK {} already downloaded as {}, serving immediately",
            lazy_pk,
            real_pk
        );
        return serve_finalized_pk(cache, &real_pk, param, content_type).await;
    }

    // 2. Télécharger en résolvant l'URL via la DB ou un provider
    let real_pk = match cache.download_lazy(lazy_pk, None).await {
        Ok(pk) => pk,
        Err(e) => {
            tracing::error!("Failed to download lazy file: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Download failed: {}", e),
            )
                .into_response();
        }
    };

    // 4. Broadcast event pour prefetch ET commutation PK
    cache.broadcast_lazy_downloaded(lazy_pk, &real_pk).await;

    // 5. Servir directement le fichier téléchargé
    serve_finalized_pk(cache, &real_pk, param, content_type).await
}

/// Fonction utilitaire pour servir un fichier avec streaming progressif
///
/// Si le fichier est en cours de téléchargement, il est streamé au fur et à mesure.
/// Sinon, le fichier complet est servi normalement.
/// Si le fichier n'existe pas et qu'un param_generator est fourni, tente de générer le param.
#[cfg(feature = "pmoserver")]
async fn serve_file_with_streaming<C: CacheConfig + 'static>(
    cache: &Arc<Cache<C>>,
    pk: &str,
    param: &str,
    content_type: &'static str,
    param_generator: Option<ParamGenerator<C>>,
) -> Response {
    // LAZY PK SUPPORT: Détecter si c'est un lazy PK
    if crate::cache::is_lazy_pk(pk) {
        return serve_lazy_audio_file(cache, pk, param, content_type).await;
    }

    let file_path = cache.get_file_path_with_qualifier(pk, param);
    let qualifier = param.to_string();

    // Si le fichier n'existe pas et qu'on a un générateur, l'utiliser
    if !file_path.exists() {
        if let Some(generator) = param_generator {
            if let Some(data) = generator(cache.clone(), pk.to_string(), param.to_string()).await {
                // Le générateur a créé les données, les servir directement
                let response =
                    (StatusCode::OK, [("content-type", content_type)], data).into_response();

                if response.status().is_success() {
                    cache.notify_broadcast(pk, &qualifier).await;
                }

                return response;
            }
        }
    }

    // Mettre à jour les stats d'utilisation
    if let Err(e) = cache.db.update_hit(pk) {
        warn!("Error updating hit count for {}: {}", pk, e);
    }

    // Vérifier si le download est en cours
    if let Some(download) = cache.get_download(pk).await {
        // Le fichier est en cours de téléchargement
        if !download.finished().await {
            // Streaming progressif
            let response = stream_file_progressive(file_path, download, content_type).await;

            if response.status().is_success() {
                cache.notify_broadcast(pk, &qualifier).await;
            }

            return response;
        }
    }

    // Fichier terminé ou pas de download en cours, servir normalement
    let response = serve_complete_file(file_path, content_type).await;

    if response.status().is_success() {
        cache.notify_broadcast(pk, &qualifier).await;
    }

    response
}

/// Stream un fichier en cours de téléchargement de manière progressive
#[cfg(feature = "pmoserver")]
async fn stream_file_progressive(
    file_path: std::path::PathBuf,
    download: Arc<crate::download::Download>,
    content_type: &'static str,
) -> Response {
    // Attendre qu'au moins 64 KB soient disponibles avant de commencer
    const MIN_SIZE_TO_START: u64 = 64 * 1024;

    if let Err(e) = download.wait_until_min_size(MIN_SIZE_TO_START).await {
        warn!("Error waiting for download to start: {}", e);
        if let Some(error_msg) = download.error().await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Download error: {}", error_msg),
            )
                .into_response();
        }
        return (StatusCode::NOT_FOUND, "File not available").into_response();
    }

    // Ouvrir le fichier en lecture
    let file = match tokio::fs::File::open(&file_path).await {
        Ok(f) => f,
        Err(e) => {
            warn!("Error opening file {:?}: {}", file_path, e);
            return (StatusCode::NOT_FOUND, "File not found").into_response();
        }
    };

    // Créer un stream à partir du fichier
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    (
        StatusCode::OK,
        [
            ("content-type", content_type),
            ("transfer-encoding", "chunked"),
        ],
        body,
    )
        .into_response()
}

/// Sert un fichier complet déjà téléchargé
#[cfg(feature = "pmoserver")]
async fn serve_complete_file(
    file_path: std::path::PathBuf,
    content_type: &'static str,
) -> Response {
    if !file_path.exists() {
        warn!("File not found: {:?}", file_path);
        return (StatusCode::NOT_FOUND, "File not found").into_response();
    }

    match tokio::fs::read(&file_path).await {
        Ok(data) => (StatusCode::OK, [("content-type", content_type)], data).into_response(),
        Err(e) => {
            warn!("Error reading file {:?}: {}", file_path, e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error reading file").into_response()
        }
    }
}

/// Crée un router pour servir les fichiers d'un cache
///
/// Crée un router avec les routes complètes incluant cache_name et cache_type.
///
/// # Arguments
///
/// * `cache` - Instance du cache
/// * `content_type` - Type MIME des fichiers (ex: "image/webp", "audio/flac")
///
/// # Routes créées
///
/// - `GET /{cache_name}/{cache_type}/{pk}` - Fichier avec param par défaut
/// - `GET /{cache_name}/{cache_type}/{pk}/{param}` - Fichier avec param spécifique
///
/// # Exemple
///
/// ```rust,no_run
/// use pmocache::pmoserver_ext;
/// use axum::Router;
/// use pmoserver::Server;
///
/// # async fn example(server: &mut Server, cache: std::sync::Arc<pmocache::Cache<CoversConfig>>) {
/// let router = pmoserver_ext::create_file_router(
///     cache.clone(),
///     "image/webp"
/// );
///
/// // Le router sera monté à la racine avec les routes complètes:
/// // GET /covers/images/{pk}
/// // GET /covers/images/{pk}/{param}
/// server.add_router("/", router).await;
/// # }
/// ```
#[cfg(feature = "pmoserver")]
pub fn create_file_router<C: CacheConfig + 'static>(
    cache: Arc<Cache<C>>,
    content_type: &'static str,
) -> Router {
    create_file_router_with_generator(cache, content_type, None)
}

/// Crée un router pour servir les fichiers d'un cache avec générateur de param
///
/// Similaire à `create_file_router` mais permet de fournir un générateur
/// pour créer des variantes à la volée (ex: redimensionnement d'images).
///
/// # Arguments
///
/// * `cache` - Instance du cache
/// * `content_type` - Type MIME des fichiers (ex: "image/webp", "audio/flac")
/// * `param_generator` - Générateur optionnel pour créer des params à la volée
///
/// # Exemple
///
/// ```rust,no_run
/// use pmocache::pmoserver_ext::{create_file_router_with_generator, ParamGenerator};
/// use std::sync::Arc;
///
/// # async fn example(cache: std::sync::Arc<pmocache::Cache<CoversConfig>>) {
/// let generator: ParamGenerator<CoversConfig> = Arc::new(|cache, pk, param| {
///     Box::pin(async move {
///         // Générer une variante si param est numérique
///         if let Ok(size) = param.parse::<usize>() {
///             // Générer et retourner les données
///             Some(vec![])
///         } else {
///             None
///         }
///     })
/// });
///
/// let router = create_file_router_with_generator(
///     cache.clone(),
///     "image/webp",
///     Some(generator)
/// );
/// # }
/// ```
#[cfg(feature = "pmoserver")]
pub fn create_file_router_with_generator<C: CacheConfig + 'static>(
    cache: Arc<Cache<C>>,
    content_type: &'static str,
    param_generator: Option<ParamGenerator<C>>,
) -> Router {
    let cache_name = C::cache_name();
    let cache_type = C::cache_type();

    let path_with_param = format!("/{}/{}/{{pk}}/{{param}}", cache_name, cache_type);
    let path_without_param = format!("/{}/{}/{{pk}}", cache_name, cache_type);

    Router::new()
        .route(&path_without_param, get(get_file::<C>))
        .route(&path_with_param, get(get_file_with_param::<C>))
        .with_state((cache, content_type, param_generator))
}

/// Crée un router pour l'API REST du cache
///
/// # Arguments
///
/// * `cache` - Instance du cache
///
/// # Routes créées
///
/// - `GET /` - Liste des items
/// - `POST /` - Ajouter un item
/// - `DELETE /` - Purger le cache
/// - `GET /{pk}` - Info d'un item
/// - `GET /{pk}/status` - Status du download
/// - `DELETE /{pk}` - Supprimer un item
/// - `POST /consolidate` - Consolider le cache
#[cfg(feature = "pmoserver")]
pub fn create_api_router<C: CacheConfig + 'static>(cache: Arc<Cache<C>>) -> Router {
    use crate::api;

    Router::new()
        .route(
            "/",
            get(api::list_items::<C>)
                .post(api::add_item::<C>)
                .delete(api::purge_cache::<C>),
        )
        .route(
            "/{pk}",
            get(api::get_item_info::<C>).delete(api::delete_item::<C>),
        )
        .route("/{pk}/status", get(api::get_download_status::<C>))
        .route("/consolidate", post(api::consolidate_cache::<C>))
        .with_state(cache)
}

/// Trait d'extension pour pmoserver::Server
///
/// Permet d'initialiser un cache générique avec routes HTTP complètes
#[cfg(feature = "pmoserver")]
pub trait GenericCacheExt {
    /// Initialise un cache générique avec routes complètes
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Répertoire de stockage du cache
    /// * `limit` - Limite de taille du cache (nombre d'éléments)
    /// * `content_type` - Type MIME des fichiers (ex: "image/webp", "audio/flac")
    ///
    /// # Routes créées
    ///
    /// - Fichiers: `/{cache_name}/{cache_type}/{pk}[/{param}]`
    /// - API: `/api/{cache_name}/*`
    /// - Swagger: `/swagger-ui/{cache_name}`
    async fn init_generic_cache<C: CacheConfig + 'static>(
        &mut self,
        cache_dir: &str,
        limit: usize,
        content_type: &'static str,
    ) -> anyhow::Result<Arc<Cache<C>>>;
}
