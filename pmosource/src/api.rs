//! # Sources API - API REST pour la gestion des sources musicales
//!
//! Ce module fournit une API REST de base pour :
//! - Lister les sources enregistrées
//! - Obtenir des informations sur une source spécifique
//! - Récupérer les statistiques d'une source
//! - Désenregistrer une source
//!
//! ## Routes
//!
//! - `GET /sources` - Liste toutes les sources
//! - `GET /sources/:id` - Informations sur une source
//! - `GET /sources/:id/capabilities` - Capacités d'une source
//! - `GET /sources/:id/statistics` - Statistiques d'une source
//! - `GET /sources/:id/root` - Container racine d'une source
//! - `GET /sources/:id/image` - Image par défaut d'une source
//! - `DELETE /sources/:id` - Désenregistrer une source
//!
//! Note: Les endpoints d'enregistrement spécifiques (POST /sources/qobuz, POST /sources/paradise)
//! sont définis dans le crate pmomediaserver pour éviter les dépendances circulaires.

#[cfg(feature = "server")]
use axum::{
    extract::{Path, Query},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};

#[cfg(feature = "server")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use crate::{
    AudioFormat, CacheStatus, MusicSource, MusicSourceError, SourceCapabilities, SourceStatistics,
};

#[cfg(feature = "server")]
use std::sync::Arc;

#[cfg(feature = "server")]
use pmodidl::{Container as DidlContainer, Item as DidlItem, Resource as DidlResource};

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
    /// Indique si le container est searchable ("1" ou "0")
    pub searchable: Option<String>,
}

/// Message d'erreur
#[cfg(feature = "server")]
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    /// Message d'erreur
    pub error: String,
}

/// Paramètres de navigation pour `browse`
#[cfg(feature = "server")]
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BrowseParams {
    /// ID de l'objet à parcourir (container ou item)
    #[serde(default)]
    pub object_id: Option<String>,

    /// Index de départ (pagination)
    #[serde(default)]
    pub starting_index: Option<usize>,

    /// Nombre d'éléments demandés (0 = tous)
    #[serde(default)]
    pub requested_count: Option<usize>,
}

/// Réponse JSON pour un browse de source
#[cfg(feature = "server")]
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SourceBrowseResponse {
    /// ObjectID parcouru
    pub object_id: String,
    /// Containers renvoyés
    pub containers: Vec<BrowseContainerInfo>,
    /// Items renvoyés
    pub items: Vec<BrowseItemInfo>,
    /// Nombre de containers retournés
    pub returned_containers: usize,
    /// Nombre d'items retournés
    pub returned_items: usize,
    /// Nombre total combiné containers + items
    pub total: usize,
    /// Update ID de la source
    pub update_id: u32,
}

/// Informations simplifiées de container pour l'API browse
#[cfg(feature = "server")]
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BrowseContainerInfo {
    /// ID du container
    pub id: String,
    /// ID du parent
    pub parent_id: String,
    /// Titre du container
    pub title: String,
    /// Classe UPnP
    pub class: String,
    /// Nombre d'enfants (si connu)
    pub child_count: Option<String>,
    /// Flag restricted
    pub restricted: Option<String>,
}

/// Informations sur une ressource audio
#[cfg(feature = "server")]
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BrowseItemResourceInfo {
    pub url: String,
    pub protocol_info: String,
    pub duration: Option<String>,
}

/// Informations simplifiées d'item audio
#[cfg(feature = "server")]
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BrowseItemInfo {
    pub id: String,
    pub parent_id: String,
    pub title: String,
    pub class: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub creator: Option<String>,
    pub album_art: Option<String>,
    pub resources: Vec<BrowseItemResourceInfo>,
}

/// Paramètres génériques pour cibler un objet d'une source
#[cfg(feature = "server")]
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ObjectQuery {
    /// ID de l'objet (container ou item)
    pub object_id: String,
}

/// Réponse pour la résolution d'URI d'un objet
#[cfg(feature = "server")]
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ResolveUriResponse {
    /// ID de l'objet demandé
    pub object_id: String,
    /// URI résolue (cache ou origine)
    pub uri: String,
}

/// États possibles pour le cache
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CacheStatusState {
    NotCached,
    Caching,
    Cached,
    Failed,
}

/// Informations détaillées sur le cache d'un objet
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct CacheStatusInfo {
    /// État du cache
    pub status: CacheStatusState,
    /// Progression (0.0 - 1.0)
    pub progress: Option<f32>,
    /// Taille en octets si connue
    pub size_bytes: Option<u64>,
    /// Message d'erreur éventuel
    pub error: Option<String>,
}

#[cfg(feature = "server")]
impl From<CacheStatus> for CacheStatusInfo {
    fn from(status: CacheStatus) -> Self {
        match status {
            CacheStatus::NotCached => Self {
                status: CacheStatusState::NotCached,
                progress: Some(0.0),
                size_bytes: None,
                error: None,
            },
            CacheStatus::Caching { progress } => Self {
                status: CacheStatusState::Caching,
                progress: Some(progress),
                size_bytes: None,
                error: None,
            },
            CacheStatus::Cached { size_bytes } => Self {
                status: CacheStatusState::Cached,
                progress: Some(1.0),
                size_bytes: Some(size_bytes),
                error: None,
            },
            CacheStatus::Failed { error } => Self {
                status: CacheStatusState::Failed,
                progress: None,
                size_bytes: None,
                error: Some(error),
            },
        }
    }
}

/// Corps de requête pour déclencher la mise en cache
#[cfg(feature = "server")]
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CacheRequest {
    /// ID de l'objet à mettre en cache
    pub object_id: String,
}

/// Réponse standard pour les endpoints de cache
#[cfg(feature = "server")]
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CacheStatusResponse {
    /// ID de l'objet
    pub object_id: String,
    /// Informations de cache
    pub status: CacheStatusInfo,
}

/// Paramètres pour récupérer les formats disponibles
#[cfg(feature = "server")]
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FormatsQuery {
    /// ID de l'objet ciblé
    pub object_id: String,
}

/// Description d'un format audio disponible
#[cfg(feature = "server")]
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AudioFormatInfo {
    /// Identifiant technique du format
    pub format_id: String,
    /// MIME type (`audio/flac`, `audio/mpeg`, ...)
    pub mime_type: String,
    /// Fréquence d'échantillonnage (Hz)
    pub sample_rate: Option<u32>,
    /// Profondeur de bits
    pub bit_depth: Option<u8>,
    /// Débit en kbps (lossy)
    pub bitrate: Option<u32>,
    /// Nombre de canaux
    pub channels: Option<u8>,
}

#[cfg(feature = "server")]
impl From<AudioFormat> for AudioFormatInfo {
    fn from(format: AudioFormat) -> Self {
        Self {
            format_id: format.format_id,
            mime_type: format.mime_type,
            sample_rate: format.sample_rate,
            bit_depth: format.bit_depth,
            bitrate: format.bitrate,
            channels: format.channels,
        }
    }
}

/// Réponse contenant les formats disponibles pour un objet
#[cfg(feature = "server")]
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AudioFormatsResponse {
    /// ID de l'objet
    pub object_id: String,
    /// Liste des formats supportés
    pub formats: Vec<AudioFormatInfo>,
}

#[cfg(feature = "server")]
impl From<&DidlContainer> for BrowseContainerInfo {
    fn from(container: &DidlContainer) -> Self {
        Self {
            id: container.id.clone(),
            parent_id: container.parent_id.clone(),
            title: container.title.clone(),
            class: container.class.clone(),
            child_count: container.child_count.clone(),
            restricted: container.restricted.clone(),
        }
    }
}

#[cfg(feature = "server")]
impl From<&DidlResource> for BrowseItemResourceInfo {
    fn from(res: &DidlResource) -> Self {
        Self {
            url: res.url.clone(),
            protocol_info: res.protocol_info.clone(),
            duration: res.duration.clone(),
        }
    }
}

#[cfg(feature = "server")]
impl From<&DidlItem> for BrowseItemInfo {
    fn from(item: &DidlItem) -> Self {
        Self {
            id: item.id.clone(),
            parent_id: item.parent_id.clone(),
            title: item.title.clone(),
            class: item.class.clone(),
            artist: item.artist.clone(),
            album: item.album.clone(),
            creator: item.creator.clone(),
            album_art: item.album_art.clone(),
            resources: item
                .resources
                .iter()
                .map(BrowseItemResourceInfo::from)
                .collect(),
        }
    }
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
    path = "/",
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
    path = "/{id}",
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
    path = "/{id}/capabilities",
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
    path = "/{id}/statistics",
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
        Some(source) => match source.statistics().await {
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
        },
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
    path = "/{id}/root",
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
        Some(source) => match source.root_container().await {
            Ok(container) => {
                let root = SourceRootContainer {
                    id: container.id,
                    parent_id: container.parent_id,
                    title: container.title,
                    class: container.class,
                    child_count: container.child_count,
                    searchable: container.searchable,
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
        },
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
    path = "/{id}/image",
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

/// Parcourt une source musicale (containers et items)
#[cfg(feature = "server")]
#[utoipa::path(
    get,
    path = "/{id}/browse",
    params(
        ("id" = String, Path, description = "ID de la source"),
        BrowseParams
    ),
    responses(
        (status = 200, description = "Résultat du browse", body = SourceBrowseResponse),
        (status = 404, description = "Source ou objet introuvable", body = ErrorResponse),
        (status = 500, description = "Erreur lors du browse", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn browse_source(
    Path(id): Path<String>,
    Query(params): Query<BrowseParams>,
) -> impl IntoResponse {
    match get_source(&id).await {
        Some(source) => {
            let object_id = params
                .object_id
                .clone()
                .unwrap_or_else(|| source.id().to_string());

            let offset = params.starting_index.unwrap_or(0);
            let requested = params.requested_count.unwrap_or(0);

            let browse_result = if requested > 0 {
                source.browse_paginated(&object_id, offset, requested).await
            } else if offset > 0 {
                source
                    .browse_paginated(&object_id, offset, usize::MAX)
                    .await
            } else {
                source.browse(&object_id).await
            };

            match browse_result {
                Ok(result) => {
                    let (containers_raw, items_raw) = match result {
                        crate::BrowseResult::Containers(c) => (c, Vec::new()),
                        crate::BrowseResult::Items(i) => (Vec::new(), i),
                        crate::BrowseResult::Mixed { containers, items } => (containers, items),
                    };

                    let containers: Vec<BrowseContainerInfo> = containers_raw
                        .iter()
                        .map(BrowseContainerInfo::from)
                        .collect();
                    let items: Vec<BrowseItemInfo> =
                        items_raw.iter().map(BrowseItemInfo::from).collect();

                    let returned_containers = containers.len();
                    let returned_items = items.len();
                    let total = returned_containers + returned_items;
                    let update_id = source.update_id().await;

                    let response = SourceBrowseResponse {
                        object_id,
                        containers,
                        items,
                        returned_containers,
                        returned_items,
                        total,
                        update_id,
                    };

                    (StatusCode::OK, Json(response)).into_response()
                }
                Err(crate::MusicSourceError::ObjectNotFound(_)) => (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "Object not found".to_string(),
                    }),
                )
                    .into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Browse failed: {}", e),
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

/// Résout l'URI réelle d'un objet (cache ou origine)
#[cfg(feature = "server")]
#[utoipa::path(
    get,
    path = "/{id}/resolve",
    params(
        ("id" = String, Path, description = "ID de la source"),
        ObjectQuery
    ),
    responses(
        (status = 200, description = "URI résolue", body = ResolveUriResponse),
        (status = 404, description = "Source ou objet introuvable", body = ErrorResponse),
        (status = 500, description = "Erreur lors de la résolution de l'URI", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn resolve_source_uri(
    Path(id): Path<String>,
    Query(params): Query<ObjectQuery>,
) -> impl IntoResponse {
    match get_source(&id).await {
        Some(source) => match source.resolve_uri(&params.object_id).await {
            Ok(uri) => {
                let response = ResolveUriResponse {
                    object_id: params.object_id,
                    uri,
                };
                (StatusCode::OK, Json(response)).into_response()
            }
            Err(MusicSourceError::ObjectNotFound(_)) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Object not found".to_string(),
                }),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to resolve URI: {}", e),
                }),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Source '{}' not found", id),
            }),
        )
            .into_response(),
    }
}

/// Récupère les métadonnées détaillées d'un item
#[cfg(feature = "server")]
#[utoipa::path(
    get,
    path = "/{id}/item",
    params(
        ("id" = String, Path, description = "ID de la source"),
        ObjectQuery
    ),
    responses(
        (status = 200, description = "Métadonnées de l'item", body = BrowseItemInfo),
        (status = 404, description = "Source ou objet introuvable", body = ErrorResponse),
        (status = 501, description = "Fonctionnalité non supportée", body = ErrorResponse),
        (status = 500, description = "Erreur lors de la récupération de l'item", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn get_source_item(
    Path(id): Path<String>,
    Query(params): Query<ObjectQuery>,
) -> impl IntoResponse {
    match get_source(&id).await {
        Some(source) => match source.get_item(&params.object_id).await {
            Ok(item) => {
                let item_info = BrowseItemInfo::from(&item);
                (StatusCode::OK, Json(item_info)).into_response()
            }
            Err(MusicSourceError::ObjectNotFound(_)) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Item not found".to_string(),
                }),
            )
                .into_response(),
            Err(MusicSourceError::NotSupported(msg)) => (
                StatusCode::NOT_IMPLEMENTED,
                Json(ErrorResponse { error: msg }),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get item: {}", e),
                }),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Source '{}' not found", id),
            }),
        )
            .into_response(),
    }
}

/// Stream les métadonnées d'un item en temps réel via Server-Sent Events
#[cfg(feature = "server")]
async fn stream_source_item_metadata(
    Path(id): Path<String>,
    Query(params): Query<ObjectQuery>,
) -> impl IntoResponse {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures::stream::{self, Stream};
    use std::convert::Infallible;
    use std::time::Duration;
    use tokio_stream::StreamExt as _;

    match get_source(&id).await {
        Some(source) => {
            let object_id = params.object_id.clone();

            // Create a stream that fetches metadata frequently for near-realtime updates
            let stream = stream::repeat_with(move || {
                let source = source.clone();
                let object_id = object_id.clone();
                async move {
                    match source.get_item(&object_id).await {
                        Ok(item) => {
                            let item_info = BrowseItemInfo::from(&item);
                            match serde_json::to_string(&item_info) {
                                Ok(json) => Ok(Event::default().data(json)),
                                Err(e) => Err(format!("Failed to serialize metadata: {}", e)),
                            }
                        }
                        Err(e) => Err(format!("Failed to get item: {}", e)),
                    }
                }
            })
            .then(|fut| fut)
            .throttle(Duration::from_millis(500))
            .filter_map(|result| match result {
                Ok(event) => Some(Ok::<_, Infallible>(event)),
                Err(e) => {
                    eprintln!("Error fetching metadata: {}", e);
                    None
                }
            });

            Sse::new(stream)
                .keep_alive(KeepAlive::default())
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

/// Récupère le statut du cache pour un objet
#[cfg(feature = "server")]
#[utoipa::path(
    get,
    path = "/{id}/cache/status",
    params(
        ("id" = String, Path, description = "ID de la source"),
        ObjectQuery
    ),
    responses(
        (status = 200, description = "Statut du cache", body = CacheStatusResponse),
        (status = 404, description = "Source ou objet introuvable", body = ErrorResponse),
        (status = 501, description = "Cache non supporté", body = ErrorResponse),
        (status = 500, description = "Erreur lors de la récupération du statut du cache", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn get_source_cache_status(
    Path(id): Path<String>,
    Query(params): Query<ObjectQuery>,
) -> impl IntoResponse {
    match get_source(&id).await {
        Some(source) => match source.get_cache_status(&params.object_id).await {
            Ok(status) => {
                let response = CacheStatusResponse {
                    object_id: params.object_id,
                    status: CacheStatusInfo::from(status),
                };
                (StatusCode::OK, Json(response)).into_response()
            }
            Err(MusicSourceError::ObjectNotFound(_)) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Object not found".to_string(),
                }),
            )
                .into_response(),
            Err(MusicSourceError::NotSupported(msg)) => (
                StatusCode::NOT_IMPLEMENTED,
                Json(ErrorResponse { error: msg }),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get cache status: {}", e),
                }),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Source '{}' not found", id),
            }),
        )
            .into_response(),
    }
}

/// Demande la mise en cache d'un objet
#[cfg(feature = "server")]
#[utoipa::path(
    post,
    path = "/{id}/cache",
    params(
        ("id" = String, Path, description = "ID de la source")
    ),
    request_body = CacheRequest,
    responses(
        (status = 200, description = "Requête de cache enregistrée", body = CacheStatusResponse),
        (status = 404, description = "Source ou objet introuvable", body = ErrorResponse),
        (status = 501, description = "Cache non supporté", body = ErrorResponse),
        (status = 500, description = "Erreur lors de la mise en cache", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn request_source_cache(
    Path(id): Path<String>,
    Json(payload): Json<CacheRequest>,
) -> impl IntoResponse {
    match get_source(&id).await {
        Some(source) => match source.cache_item(&payload.object_id).await {
            Ok(status) => {
                let response = CacheStatusResponse {
                    object_id: payload.object_id,
                    status: CacheStatusInfo::from(status),
                };
                (StatusCode::OK, Json(response)).into_response()
            }
            Err(MusicSourceError::ObjectNotFound(_)) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Object not found".to_string(),
                }),
            )
                .into_response(),
            Err(MusicSourceError::NotSupported(msg)) => (
                StatusCode::NOT_IMPLEMENTED,
                Json(ErrorResponse { error: msg }),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to cache item: {}", e),
                }),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Source '{}' not found", id),
            }),
        )
            .into_response(),
    }
}

/// Récupère les formats audio disponibles pour un objet
#[cfg(feature = "server")]
#[utoipa::path(
    get,
    path = "/{id}/formats",
    params(
        ("id" = String, Path, description = "ID de la source"),
        FormatsQuery
    ),
    responses(
        (status = 200, description = "Formats disponibles", body = AudioFormatsResponse),
        (status = 404, description = "Source ou objet introuvable", body = ErrorResponse),
        (status = 500, description = "Erreur lors de la récupération des formats", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn get_source_formats(
    Path(id): Path<String>,
    Query(params): Query<FormatsQuery>,
) -> impl IntoResponse {
    match get_source(&id).await {
        Some(source) => match source.get_available_formats(&params.object_id).await {
            Ok(formats) => {
                let response = AudioFormatsResponse {
                    object_id: params.object_id,
                    formats: formats.into_iter().map(AudioFormatInfo::from).collect(),
                };
                (StatusCode::OK, Json(response)).into_response()
            }
            Err(MusicSourceError::ObjectNotFound(_)) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Object not found".to_string(),
                }),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get available formats: {}", e),
                }),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Source '{}' not found", id),
            }),
        )
            .into_response(),
    }
}

/// Désenregistre une source musicale
///
/// Supprime une source du registre par son ID.
#[cfg(feature = "server")]
#[utoipa::path(
    delete,
    path = "/{id}",
    params(
        ("id" = String, Path, description = "ID de la source à supprimer")
    ),
    responses(
        (status = 200, description = "Source supprimée"),
        (status = 404, description = "Source non trouvée", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn unregister_source_handler(Path(id): Path<String>) -> impl IntoResponse {
    if unregister_source(&id).await {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": format!("Source '{}' unregistered successfully", id)
            })),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Source '{}' not found", id),
            }),
        )
            .into_response()
    }
}

/// Crée le router pour l'API des sources (endpoints de lecture uniquement)
///
/// # Returns
///
/// Un `Router` Axum avec les routes de lecture de l'API configurées.
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
///
/// Note: Les endpoints d'enregistrement spécifiques (POST /sources/qobuz, POST /sources/paradise)
/// doivent être ajoutés via pmomediaserver pour éviter les dépendances circulaires.
#[cfg(feature = "server")]
pub fn create_sources_router() -> Router {
    Router::new()
        .route("/", get(list_sources))
        .route(
            "/{id}",
            get(get_source_info).delete(unregister_source_handler),
        )
        .route("/{id}/capabilities", get(get_source_capabilities))
        .route("/{id}/statistics", get(get_source_statistics))
        .route("/{id}/root", get(get_source_root))
        .route("/{id}/browse", get(browse_source))
        .route("/{id}/image", get(get_source_image))
        .route("/{id}/item", get(get_source_item))
        .route("/{id}/item/stream", get(stream_source_item_metadata))
        .route("/{id}/resolve", get(resolve_source_uri))
        .route("/{id}/cache/status", get(get_source_cache_status))
        .route("/{id}/cache", post(request_source_cache))
        .route("/{id}/formats", get(get_source_formats))
}

/// Structure pour la documentation OpenAPI de base
///
/// Note: Cette documentation couvre les endpoints de base uniquement.
/// Les endpoints d'enregistrement spécifiques (Qobuz, Paradise) sont documentés
/// dans le crate pmomediaserver.
#[cfg(feature = "server")]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        list_sources,
        get_source_info,
        get_source_capabilities,
        get_source_statistics,
        get_source_root,
        browse_source,
        get_source_image,
        get_source_item,
        resolve_source_uri,
        get_source_cache_status,
        request_source_cache,
        get_source_formats,
        unregister_source_handler,
    ),
    components(
        schemas(
            SourceInfo,
            SourceCapabilitiesInfo,
            SourceStatisticsInfo,
            SourcesList,
            SourceRootContainer,
            BrowseContainerInfo,
            BrowseItemResourceInfo,
            BrowseItemInfo,
            SourceBrowseResponse,
            ResolveUriResponse,
            CacheStatusState,
            CacheStatusInfo,
            CacheStatusResponse,
            CacheRequest,
            AudioFormatInfo,
            AudioFormatsResponse,
            ErrorResponse,
        )
    ),
    tags(
        (name = "sources", description = "API de gestion des sources musicales (base)")
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
