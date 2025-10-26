use crate::Config;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_yaml::Value;
use std::sync::Arc;

/// Structure pour récupérer une valeur de configuration
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConfigValue {
    /// Chemin de la clé (ex: "host.http_port")
    pub path: String,
    /// Valeur au format JSON
    pub value: JsonValue,
}

/// Structure pour mettre à jour une valeur de configuration
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateConfigRequest {
    /// Chemin de la clé (ex: "host.http_port")
    pub path: String,
    /// Nouvelle valeur au format JSON
    pub value: JsonValue,
}

/// Structure pour la réponse d'une mise à jour
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateConfigResponse {
    pub success: bool,
    pub message: String,
}

/// Erreur API
#[derive(Debug)]
pub struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": self.0.to_string()
            })),
        )
            .into_response()
    }
}

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

/// GET /api/config - Récupérer toute la configuration
#[utoipa::path(
    get,
    path = "/api/config",
    tag = "config",
    responses(
        (status = 200, description = "Configuration complète", body = serde_json::Value)
    )
)]
async fn get_full_config(State(config): State<Arc<Config>>) -> Result<Json<JsonValue>, ApiError> {
    let value = config.get_value(&[])?;
    let json_value = yaml_to_json(&value)?;
    Ok(Json(json_value))
}

/// GET /api/config/{path} - Récupérer une valeur à un chemin spécifique
#[utoipa::path(
    get,
    path = "/api/config/{path}",
    tag = "config",
    params(
        ("path" = String, Path, description = "Chemin de la configuration (séparé par des points, ex: host.http_port)")
    ),
    responses(
        (status = 200, description = "Valeur de configuration", body = ConfigValue),
        (status = 404, description = "Chemin non trouvé")
    )
)]
async fn get_config_value(
    State(config): State<Arc<Config>>,
    Path(path): Path<String>,
) -> Result<Json<ConfigValue>, ApiError> {
    let path_parts: Vec<&str> = path.split('.').collect();
    let value = config.get_value(&path_parts)?;
    let json_value = yaml_to_json(&value)?;

    Ok(Json(ConfigValue {
        path,
        value: json_value,
    }))
}

/// POST /api/config - Mettre à jour une valeur de configuration
#[utoipa::path(
    post,
    path = "/api/config",
    tag = "config",
    request_body = UpdateConfigRequest,
    responses(
        (status = 200, description = "Configuration mise à jour", body = UpdateConfigResponse)
    )
)]
async fn update_config_value(
    State(config): State<Arc<Config>>,
    Json(request): Json<UpdateConfigRequest>,
) -> Result<Json<UpdateConfigResponse>, ApiError> {
    let path_parts: Vec<&str> = request.path.split('.').collect();
    let yaml_value = json_to_yaml(&request.value)?;

    config.set_value(&path_parts, yaml_value)?;

    Ok(Json(UpdateConfigResponse {
        success: true,
        message: format!("Configuration updated at path: {}", request.path),
    }))
}

/// Convertit une valeur YAML en JSON
fn yaml_to_json(yaml: &Value) -> Result<JsonValue, ApiError> {
    // Serialize YAML to string then parse as JSON
    let yaml_str = serde_yaml::to_string(yaml)?;
    Ok(serde_json::from_str(&yaml_str)?)
}

/// Convertit une valeur JSON en YAML
fn json_to_yaml(json: &JsonValue) -> Result<Value, ApiError> {
    // Serialize JSON to string then parse as YAML
    let json_str = serde_json::to_string(json)?;
    Ok(serde_yaml::from_str(&json_str)?)
}

/// Crée le router API pour la configuration
pub fn create_router(config: Arc<Config>) -> Router {
    Router::new()
        .route("/api/config", get(get_full_config))
        .route("/api/config", post(update_config_value))
        .route("/api/config/:path", get(get_config_value))
        .with_state(config)
}
