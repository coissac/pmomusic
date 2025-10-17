//! # Sources API - API REST pour la gestion des sources musicales
//!
//! Ce module fournit une API REST pour :
//! - Lister les sources enregistrées
//! - Obtenir des informations sur une source spécifique
//! - Récupérer les statistiques d'une source
//!
//! ## Routes
//!
//! - `GET /sources` - Liste toutes les sources
//! - `GET /sources/:id` - Informations sur une source
//! - `GET /sources/:id/capabilities` - Capacités d'une source
//! - `GET /sources/:id/statistics` - Statistiques d'une source
//! - `GET /sources/:id/root` - Container racine d'une source
//! - `GET /sources/:id/image` - Image par défaut d'une source

#[cfg(feature = "server")]
use axum::{
    extract::Path,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};

#[cfg(feature = "server")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use crate::{MusicSource, SourceCapabilities, SourceStatistics};

#[cfg(feature = "server")]
use std::sync::Arc;

/// Information sur une source musicale
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SourceInfo {
    /// ID unique de la source
    pub id: String,
    /// Nom de la source
    pub name: String,
    /// La source supporte-t-elle les opérations FIFO
    pub supports_fifo: bool,
    /// Capacités de la source
    pub capabilities: SourceCapabilitiesInfo,
}

/// Capacités d'une source
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SourceCapabilitiesInfo {
    pub supports_search: bool,
    pub supports_favorites: bool,
    pub supports_playlists: bool,
    pub supports_user_content: bool,
    pub supports_high_res_audio: bool,
    pub max_sample_rate: Option<u32>,
    pub supports_multiple_formats: bool,
    pub supports_advanced_search: bool,
    pub supports_pagination: bool,
}

#[cfg(feature = "server")]
impl From<SourceCapabilities> for SourceCapabilitiesInfo {
    fn from(caps: SourceCapabilities) -> Self {
        Self {
            supports_search: caps.supports_search,
            supports_favorites: caps.supports_favorites,
            supports_playlists: caps.supports_playlists,
            supports_user_content: caps.supports_user_content,
            supports_high_res_audio: caps.supports_high_res_audio,
            max_sample_rate: caps.max_sample_rate,
            supports_multiple_formats: caps.supports_multiple_formats,
            supports_advanced_search: caps.supports_advanced_search,
            supports_pagination: caps.supports_pagination,
        }
    }
}

/// Statistiques d'une source
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SourceStatisticsInfo {
    pub total_items: Option<usize>,
    pub total_containers: Option<usize>,
    pub cached_items: Option<usize>,
    pub cache_size_bytes: Option<u64>,
}

#[cfg(feature = "server")]
impl From<SourceStatistics> for SourceStatisticsInfo {
    fn from(stats: SourceStatistics) -> Self {
        Self {
            total_items: stats.total_items,
            total_containers: stats.total_containers,
            cached_items: stats.cached_items,
            cache_size_bytes: stats.cache_size_bytes,
        }
    }
}

/// Liste des sources enregistrées
#[cfg(feature = "server")]
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SourcesList {
    /// Nombre total de sources
    pub count: usize,
    /// Liste des sources
    pub sources: Vec<SourceInfo>,
}

/// Container racine d'une source
#[cfg(feature = "server")]
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SourceRootContainer {
    /// ID du container
    pub id: String,
    /// Parent ID
    pub parent_id: String,
    /// Titre du container
    pub title: String,
    /// Classe UPnP
    pub class: String,
    /// Nombre d'enfants
    pub child_count: Option<String>,
}

/// Message d'erreur
#[cfg(feature = "server")]
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    /// Message d'erreur
    pub error: String,
}

// ============= Gestionnaire de registre global =============

#[cfg(feature = "server")]
use tokio::sync::RwLock;

#[cfg(feature = "server")]
lazy_static::lazy_static! {
    static ref SOURCE_REGISTRY: Arc<RwLock<Vec<Arc<dyn MusicSource>>>> =
        Arc::new(RwLock::new(Vec::new()));
}

/// Enregistre une source dans le registre global
#[cfg(feature = "server")]
pub async fn register_source(source: Arc<dyn MusicSource>) {
    let mut registry = SOURCE_REGISTRY.write().await;

    // Vérifier si la source existe déjà (par ID)
    let source_id = source.id();
    registry.retain(|s| s.id() != source_id);

    // Ajouter la nouvelle source
    registry.push(source);
}

/// Retire une source du registre global
#[cfg(feature = "server")]
pub async fn unregister_source(source_id: &str) -> bool {
    let mut registry = SOURCE_REGISTRY.write().await;
    let initial_len = registry.len();
    registry.retain(|s| s.id() != source_id);
    registry.len() < initial_len
}

/// Liste toutes les sources enregistrées
#[cfg(feature = "server")]
pub async fn list_all_sources() -> Vec<Arc<dyn MusicSource>> {
    let registry = SOURCE_REGISTRY.read().await;
    registry.clone()
}

/// Récupère une source par son ID
#[cfg(feature = "server")]
pub async fn get_source(source_id: &str) -> Option<Arc<dyn MusicSource>> {
    let registry = SOURCE_REGISTRY.read().await;
    registry.iter().find(|s| s.id() == source_id).cloned()
}

// ============= Handlers API =============

/// Liste toutes les sources musicales enregistrées
///
/// Retourne la liste complète des sources avec leurs informations.
#[cfg(feature = "server")]
#[utoipa::path(
    get,
    path = "/sources",
    responses(
        (status = 200, description = "Liste des sources", body = SourcesList),
    ),
    tag = "sources"
)]
async fn list_sources() -> impl IntoResponse {
    let sources = list_all_sources().await;

    let source_infos: Vec<SourceInfo> = sources
        .iter()
        .map(|s| {
            let caps = s.capabilities();
            SourceInfo {
                id: s.id().to_string(),
                name: s.name().to_string(),
                supports_fifo: s.supports_fifo(),
                capabilities: caps.into(),
            }
        })
        .collect();

    let list = SourcesList {
        count: source_infos.len(),
        sources: source_infos,
    };

    Json(list)
}

/// Obtient les informations d'une source spécifique
///
/// Retourne les détails d'une source musicale par son ID.
#[cfg(feature = "server")]
#[utoipa::path(
    get,
    path = "/sources/{id}",
    params(
        ("id" = String, Path, description = "ID de la source")
    ),
    responses(
        (status = 200, description = "Informations de la source", body = SourceInfo),
        (status = 404, description = "Source non trouvée", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn get_source_info(Path(id): Path<String>) -> impl IntoResponse {
    match get_source(&id).await {
        Some(source) => {
            let caps = source.capabilities();
            let info = SourceInfo {
                id: source.id().to_string(),
                name: source.name().to_string(),
                supports_fifo: source.supports_fifo(),
                capabilities: caps.into(),
            };
            (StatusCode::OK, Json(info)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Source '{}' not found", id),
            }),
        )
            .into_response(),
    }
}

/// Obtient les capacités d'une source
///
/// Retourne les capacités détaillées d'une source musicale.
#[cfg(feature = "server")]
#[utoipa::path(
    get,
    path = "/sources/{id}/capabilities",
    params(
        ("id" = String, Path, description = "ID de la source")
    ),
    responses(
        (status = 200, description = "Capacités de la source", body = SourceCapabilitiesInfo),
        (status = 404, description = "Source non trouvée", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn get_source_capabilities(Path(id): Path<String>) -> impl IntoResponse {
    match get_source(&id).await {
        Some(source) => {
            let caps: SourceCapabilitiesInfo = source.capabilities().into();
            (StatusCode::OK, Json(caps)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Source '{}' not found", id),
            }),
        )
            .into_response(),
    }
}

/// Obtient les statistiques d'une source
///
/// Retourne les statistiques d'une source musicale.
#[cfg(feature = "server")]
#[utoipa::path(
    get,
    path = "/sources/{id}/statistics",
    params(
        ("id" = String, Path, description = "ID de la source")
    ),
    responses(
        (status = 200, description = "Statistiques de la source", body = SourceStatisticsInfo),
        (status = 404, description = "Source non trouvée", body = ErrorResponse),
        (status = 500, description = "Erreur lors de la récupération des statistiques", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn get_source_statistics(Path(id): Path<String>) -> impl IntoResponse {
    match get_source(&id).await {
        Some(source) => {
            match source.statistics().await {
                Ok(stats) => {
                    let stats_info: SourceStatisticsInfo = stats.into();
                    (StatusCode::OK, Json(stats_info)).into_response()
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to get statistics: {}", e),
                    }),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Source '{}' not found", id),
            }),
        )
            .into_response(),
    }
}

/// Obtient le container racine d'une source
///
/// Retourne le container racine d'une source musicale.
#[cfg(feature = "server")]
#[utoipa::path(
    get,
    path = "/sources/{id}/root",
    params(
        ("id" = String, Path, description = "ID de la source")
    ),
    responses(
        (status = 200, description = "Container racine de la source", body = SourceRootContainer),
        (status = 404, description = "Source non trouvée", body = ErrorResponse),
        (status = 500, description = "Erreur lors de la récupération du container", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn get_source_root(Path(id): Path<String>) -> impl IntoResponse {
    match get_source(&id).await {
        Some(source) => {
            match source.root_container().await {
                Ok(container) => {
                    let root = SourceRootContainer {
                        id: container.id,
                        parent_id: container.parent_id,
                        title: container.title,
                        class: container.class,
                        child_count: container.child_count,
                    };
                    (StatusCode::OK, Json(root)).into_response()
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to get root container: {}", e),
                    }),
                )
                    .into_response(),
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Source '{}' not found", id),
            }),
        )
            .into_response(),
    }
}

/// Obtient l'image par défaut d'une source
///
/// Retourne l'image/logo par défaut d'une source en format WebP.
#[cfg(feature = "server")]
#[utoipa::path(
    get,
    path = "/sources/{id}/image",
    params(
        ("id" = String, Path, description = "ID de la source")
    ),
    responses(
        (status = 200, description = "Image de la source", content_type = "image/webp"),
        (status = 404, description = "Source non trouvée", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn get_source_image(Path(id): Path<String>) -> Response {
    match get_source(&id).await {
        Some(source) => {
            // Copier les données de l'image pour respecter les lifetime requirements
            let image_data = source.default_image().to_vec();
            let mime_type = source.default_image_mime_type().to_string();

            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime_type.as_str())],
                image_data,
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Source '{}' not found", id),
            }),
        )
            .into_response(),
    }
}

/// Crée le router pour l'API des sources
///
/// # Returns
///
/// Un `Router` Axum avec toutes les routes de l'API configurées.
///
/// # Examples
///
/// ```ignore
/// use pmosource::api::create_sources_router;
/// use axum::Router;
///
/// let app = Router::new()
///     .nest("/api", create_sources_router());
/// ```
#[cfg(feature = "server")]
pub fn create_sources_router() -> Router {
    Router::new()
        .route("/sources", get(list_sources))
        .route("/sources/{id}", get(get_source_info))
        .route("/sources/{id}/capabilities", get(get_source_capabilities))
        .route("/sources/{id}/statistics", get(get_source_statistics))
        .route("/sources/{id}/root", get(get_source_root))
        .route("/sources/{id}/image", get(get_source_image))
}

/// Structure pour la documentation OpenAPI
#[cfg(feature = "server")]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        list_sources,
        get_source_info,
        get_source_capabilities,
        get_source_statistics,
        get_source_root,
        get_source_image,
    ),
    components(
        schemas(
            SourceInfo,
            SourceCapabilitiesInfo,
            SourceStatisticsInfo,
            SourcesList,
            SourceRootContainer,
            ErrorResponse,
        )
    ),
    tags(
        (name = "sources", description = "API de gestion des sources musicales")
    )
)]
pub struct SourcesApiDoc;

#[cfg(test)]
mod tests {
    #[test]
    fn test_api_module_compiles() {
        // Ce test vérifie simplement que le module compile
    }
}
