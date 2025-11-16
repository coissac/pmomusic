//! # Sources Registration API - Endpoints d'enregistrement dynamique des sources
//!
//! Ce module étend l'API de base de `pmosource` avec des endpoints d'enregistrement
//! spécifiques pour chaque type de source musicale (Qobuz, Paradise, etc.).
//!
//! ## Routes additionnelles
//!
//! - `POST /sources/qobuz` - Enregistrer Qobuz (feature "qobuz")
//! - `POST /sources/paradise` - Enregistrer Radio Paradise (feature "paradise")
//!
//! ## Architecture
//!
//! Ces endpoints sont définis ici plutôt que dans `pmosource` pour éviter les
//! dépendances circulaires (pmoqobuz et pmoparadise dépendent de pmosource).

use axum::{Router, extract::Json, http::StatusCode, response::IntoResponse, routing::post};
use pmosource::MusicSource;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Credentials pour Qobuz
#[cfg(feature = "qobuz")]
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct QobuzCredentials {
    /// Nom d'utilisateur Qobuz (optionnel, lu depuis la config si absent)
    pub username: Option<String>,
    /// Mot de passe Qobuz (optionnel, lu depuis la config si absent)
    pub password: Option<String>,
}

/// Paramètres pour Radio Paradise
#[cfg(feature = "paradise")]
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ParadiseParams {
    /// Capacité FIFO (optionnelle, 50 par défaut)
    #[serde(default)]
    pub fifo_capacity: Option<usize>,

    /// URL de base du serveur (optionnelle, "http://localhost:8080" par défaut)
    #[serde(default)]
    pub base_url: Option<String>,
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
    use pmosource::api::register_source;

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

    // Créer et enregistrer la source depuis le registry
    let source = match QobuzSource::from_registry(client) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to create source: {}", e),
                }),
            )
                .into_response();
        }
    };
    let source_id = source.as_ref().id().to_string();

    register_source(source).await;

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
    use pmosource::api::register_source;

    // Utiliser l'URL de base depuis les params ou une valeur par défaut
    let base_url = params
        .base_url
        .unwrap_or_else(|| "http://localhost:8080".to_string());

    // Créer la source Radio Paradise (utilise le singleton PlaylistManager)
    let source = Arc::new(RadioParadiseSource::new(base_url));

    let source_id = source.as_ref().id().to_string();

    register_source(source).await;

    (
        StatusCode::CREATED,
        Json(SourceRegisteredResponse {
            message: "Radio Paradise source registered successfully".to_string(),
            source_id,
        }),
    )
        .into_response()
}

/// Crée un router avec les endpoints d'enregistrement des sources
///
/// Ce router doit être combiné avec le router de base de `pmosource::api::create_sources_router()`
/// pour obtenir une API complète.
///
/// # Examples
///
/// ```ignore
/// use pmomediaserver::sources_api::create_registration_router;
/// use pmosource::api::create_sources_router;
/// use axum::Router;
///
/// let sources_router = create_sources_router();
/// let registration_router = create_registration_router();
/// let combined = sources_router.merge(registration_router);
/// ```
pub fn create_registration_router() -> Router {
    let mut router = Router::new();

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

/// Structure pour la documentation OpenAPI des endpoints d'enregistrement (Qobuz + Paradise)
#[cfg(all(feature = "qobuz", feature = "paradise"))]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        register_qobuz,
        register_paradise,
    ),
    components(
        schemas(
            SourceRegisteredResponse,
            ErrorResponse,
            QobuzCredentials,
            ParadiseParams,
        )
    ),
    tags(
        (name = "sources", description = "Enregistrement dynamique des sources musicales")
    )
)]
pub struct SourceRegistrationApiDoc;

/// Structure pour la documentation OpenAPI (Qobuz uniquement)
#[cfg(all(feature = "qobuz", not(feature = "paradise")))]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        register_qobuz,
    ),
    components(
        schemas(
            SourceRegisteredResponse,
            ErrorResponse,
            QobuzCredentials,
        )
    ),
    tags(
        (name = "sources", description = "Enregistrement dynamique des sources musicales")
    )
)]
pub struct SourceRegistrationApiDoc;

/// Structure pour la documentation OpenAPI (Paradise uniquement)
#[cfg(all(feature = "paradise", not(feature = "qobuz")))]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        register_paradise,
    ),
    components(
        schemas(
            SourceRegisteredResponse,
            ErrorResponse,
            ParadiseParams,
        )
    ),
    tags(
        (name = "sources", description = "Enregistrement dynamique des sources musicales")
    )
)]
pub struct SourceRegistrationApiDoc;
