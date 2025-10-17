//! # Sources API - API REST pour la gestion des sources musicales
//!
//! Ce module fournit une API REST pour :
//! - Lister les sources enregistrées
//! - Obtenir des informations sur une source spécifique
//! - Enregistrer/désenregistrer des sources
//!
//! ## Routes
//!
//! - `GET /sources` - Liste toutes les sources
//! - `GET /sources/:id` - Informations sur une source
//! - `POST /sources/qobuz` - Enregistrer Qobuz (feature "qobuz")
//! - `DELETE /sources/:id` - Désenregistrer une source

use crate::server_ext::get_source_registry;
use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    Json, Router,
    routing::{delete, get, post},
};
use pmosource::MusicSource;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Information sur une source musicale
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SourceCapabilitiesInfo {
    pub supports_search: bool,
    pub supports_favorites: bool,
    pub supports_playlists: bool,
    pub supports_high_res_audio: bool,
}

/// Liste des sources enregistrées
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SourcesList {
    /// Nombre total de sources
    pub count: usize,
    /// Liste des sources
    pub sources: Vec<SourceInfo>,
}

/// Credentials pour Qobuz
#[cfg(feature = "qobuz")]
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct QobuzCredentials {
    /// Nom d'utilisateur Qobuz (optionnel, lu depuis la config si absent)
    pub username: Option<String>,
    /// Mot de passe Qobuz (optionnel, lu depuis la config si absent)
    pub password: Option<String>,
}

/// Paramètres pour Radio Paradise (actuellement vide, mais peut être étendu)
#[cfg(feature = "paradise")]
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ParadiseParams {
    /// Capacité FIFO (optionnelle, 50 par défaut)
    #[serde(default)]
    pub fifo_capacity: Option<usize>,
}

/// Réponse d'enregistrement de source
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SourceRegisteredResponse {
    /// Message de succès
    pub message: String,
    /// ID de la source enregistrée
    pub source_id: String,
}

/// Message d'erreur
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    /// Message d'erreur
    pub error: String,
}

/// Liste toutes les sources musicales enregistrées
///
/// Retourne la liste complète des sources avec leurs informations.
#[utoipa::path(
    get,
    path = "/sources",
    responses(
        (status = 200, description = "Liste des sources", body = SourcesList),
    ),
    tag = "sources"
)]
async fn list_sources() -> impl IntoResponse {
    let registry = get_source_registry().await;
    let sources = registry.list_all().await;

    let source_infos: Vec<SourceInfo> = sources
        .iter()
        .map(|s| {
            let caps = s.capabilities();
            SourceInfo {
                id: s.id().to_string(),
                name: s.name().to_string(),
                supports_fifo: s.supports_fifo(),
                capabilities: SourceCapabilitiesInfo {
                    supports_search: caps.supports_search,
                    supports_favorites: caps.supports_favorites,
                    supports_playlists: caps.supports_playlists,
                    supports_high_res_audio: caps.supports_high_res_audio,
                },
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
async fn get_source(Path(id): Path<String>) -> impl IntoResponse {
    let registry = get_source_registry().await;

    match registry.get(&id).await {
        Some(source) => {
            let caps = source.capabilities();
            let info = SourceInfo {
                id: source.id().to_string(),
                name: source.name().to_string(),
                supports_fifo: source.supports_fifo(),
                capabilities: SourceCapabilitiesInfo {
                    supports_search: caps.supports_search,
                    supports_favorites: caps.supports_favorites,
                    supports_playlists: caps.supports_playlists,
                    supports_high_res_audio: caps.supports_high_res_audio,
                },
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

/// Enregistre une source Qobuz
///
/// Enregistre une nouvelle source Qobuz avec les credentials fournis ou depuis la config.
#[cfg(feature = "qobuz")]
#[utoipa::path(
    post,
    path = "/sources/qobuz",
    request_body = QobuzCredentials,
    responses(
        (status = 201, description = "Source enregistrée", body = SourceRegisteredResponse),
        (status = 400, description = "Erreur d'enregistrement", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn register_qobuz(Json(creds): Json<QobuzCredentials>) -> impl IntoResponse {
    use pmoqobuz::{QobuzClient, QobuzSource};

    let registry = get_source_registry().await;

    // Créer le client selon les credentials fournis
    let client_result = if let (Some(username), Some(password)) = (creds.username, creds.password) {
        QobuzClient::new(&username, &password).await
    } else {
        QobuzClient::from_config().await
    };

    let client = match client_result {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Failed to create Qobuz client: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Récupérer l'URL de base du serveur depuis la config
    let config = pmoconfig::get_config();
    let port = config.get_http_port();
    let base_url = format!("http://localhost:{}", port);

    // Créer et enregistrer la source
    let source = Arc::new(QobuzSource::new(client, &base_url));
    let source_id = source.as_ref().id().to_string();

    registry.register(source).await;

    (
        StatusCode::CREATED,
        Json(SourceRegisteredResponse {
            message: "Qobuz source registered successfully".to_string(),
            source_id,
        }),
    )
        .into_response()
}

/// Enregistre une source Radio Paradise
///
/// Enregistre une nouvelle source Radio Paradise (ne nécessite pas d'authentification).
#[cfg(feature = "paradise")]
#[utoipa::path(
    post,
    path = "/sources/paradise",
    request_body = ParadiseParams,
    responses(
        (status = 201, description = "Source enregistrée", body = SourceRegisteredResponse),
        (status = 400, description = "Erreur d'enregistrement", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn register_paradise(Json(params): Json<ParadiseParams>) -> impl IntoResponse {
    use pmoparadise::{RadioParadiseClient, RadioParadiseSource};

    let registry = get_source_registry().await;

    // Créer le client (Radio Paradise ne nécessite pas d'auth)
    let client = match RadioParadiseClient::new().await {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Failed to create Radio Paradise client: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Récupérer l'URL de base du serveur depuis la config
    let config = pmoconfig::get_config();
    let port = config.get_http_port();
    let base_url = format!("http://localhost:{}", port);

    // Créer et enregistrer la source
    let source = if let Some(capacity) = params.fifo_capacity {
        Arc::new(RadioParadiseSource::new(client, &base_url, capacity))
    } else {
        Arc::new(RadioParadiseSource::new_default(client, &base_url))
    };

    let source_id = source.as_ref().id().to_string();

    registry.register(source).await;

    (
        StatusCode::CREATED,
        Json(SourceRegisteredResponse {
            message: "Radio Paradise source registered successfully".to_string(),
            source_id,
        }),
    )
        .into_response()
}

/// Désenregistre une source musicale
///
/// Supprime une source du registre par son ID.
#[utoipa::path(
    delete,
    path = "/sources/{id}",
    params(
        ("id" = String, Path, description = "ID de la source à supprimer")
    ),
    responses(
        (status = 200, description = "Source supprimée"),
        (status = 404, description = "Source non trouvée", body = ErrorResponse),
    ),
    tag = "sources"
)]
async fn unregister_source(Path(id): Path<String>) -> impl IntoResponse {
    let registry = get_source_registry().await;

    if registry.remove(&id).await {
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

/// Crée le router pour l'API des sources
///
/// # Returns
///
/// Un `Router` Axum avec toutes les routes de l'API configurées.
///
/// # Examples
///
/// ```ignore
/// use pmomediaserver::sources_api::sources_api_router;
/// use axum::Router;
///
/// let app = Router::new()
///     .nest("/api", sources_api_router());
/// ```
pub fn sources_api_router() -> Router {
    let mut router = Router::new()
        .route("/sources", get(list_sources))
        .route("/sources/{id}", get(get_source))
        .route("/sources/{id}", delete(unregister_source));

    #[cfg(feature = "qobuz")]
    {
        router = router.route("/sources/qobuz", post(register_qobuz));
    }

    #[cfg(feature = "paradise")]
    {
        router = router.route("/sources/paradise", post(register_paradise));
    }

    router
}

/// Structure pour la documentation OpenAPI (Qobuz + Paradise)
#[cfg(all(feature = "qobuz", feature = "paradise"))]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        list_sources,
        get_source,
        unregister_source,
        register_qobuz,
        register_paradise,
    ),
    components(
        schemas(
            SourceInfo,
            SourceCapabilitiesInfo,
            SourcesList,
            SourceRegisteredResponse,
            ErrorResponse,
            QobuzCredentials,
            ParadiseParams,
        )
    ),
    tags(
        (name = "sources", description = "Gestion des sources musicales")
    )
)]
pub struct SourcesApiDoc;

/// Structure pour la documentation OpenAPI (Qobuz uniquement)
#[cfg(all(feature = "qobuz", not(feature = "paradise")))]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        list_sources,
        get_source,
        unregister_source,
        register_qobuz,
    ),
    components(
        schemas(
            SourceInfo,
            SourceCapabilitiesInfo,
            SourcesList,
            SourceRegisteredResponse,
            ErrorResponse,
            QobuzCredentials,
        )
    ),
    tags(
        (name = "sources", description = "Gestion des sources musicales")
    )
)]
pub struct SourcesApiDoc;

/// Structure pour la documentation OpenAPI (Paradise uniquement)
#[cfg(all(feature = "paradise", not(feature = "qobuz")))]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        list_sources,
        get_source,
        unregister_source,
        register_paradise,
    ),
    components(
        schemas(
            SourceInfo,
            SourceCapabilitiesInfo,
            SourcesList,
            SourceRegisteredResponse,
            ErrorResponse,
            ParadiseParams,
        )
    ),
    tags(
        (name = "sources", description = "Gestion des sources musicales")
    )
)]
pub struct SourcesApiDoc;

/// Structure pour la documentation OpenAPI (sans sources spécifiques)
#[cfg(not(any(feature = "qobuz", feature = "paradise")))]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        list_sources,
        get_source,
        unregister_source,
    ),
    components(
        schemas(
            SourceInfo,
            SourceCapabilitiesInfo,
            SourcesList,
            SourceRegisteredResponse,
            ErrorResponse,
        )
    ),
    tags(
        (name = "sources", description = "Gestion des sources musicales")
    )
)]
pub struct SourcesApiDoc;
