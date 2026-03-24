//! Handlers HTTP pour l'enregistrement/désenregistrement des instances WebRenderer.
//!
//! - POST /api/webrenderer/register  → crée ou reconnecte une instance
//! - DELETE /api/webrenderer/{id}    → désenregistrement explicite

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::registry::RendererRegistry;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub instance_id: String,
    pub user_agent: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub stream_url: String,
    pub udn: String,
}

/// POST /api/webrenderer/register
pub async fn register_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    tracing::info!(
        instance_id = %req.instance_id,
        user_agent = %req.user_agent,
        "WebRenderer: register request"
    );

    match registry
        .register_or_reconnect(&req.instance_id, &req.user_agent)
        .await
    {
        Ok((stream_url, udn)) => {
            tracing::info!(
                instance_id = %req.instance_id,
                stream_url = %stream_url,
                udn = %udn,
                "WebRenderer: registered"
            );
            (StatusCode::OK, Json(RegisterResponse { stream_url, udn })).into_response()
        }
        Err(e) => {
            tracing::error!(
                instance_id = %req.instance_id,
                error = %e,
                "WebRenderer: registration failed"
            );
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PositionUpdateRequest {
    pub position_sec: f64,
    pub duration_sec: Option<f64>,
}

/// POST /api/webrenderer/{id}/position
/// position_sec est ignoré (géré par PlayerEvent::Position côté serveur).
/// duration_sec est utilisé comme fallback si la source ne connaît pas la durée (flux radio).
pub async fn position_update_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
    Json(req): Json<PositionUpdateRequest>,
) -> impl IntoResponse {
    registry.update_duration(&instance_id, req.duration_sec);
    StatusCode::NO_CONTENT
}

/// DELETE /api/webrenderer/{id}
pub async fn unregister_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    tracing::info!(instance_id = %instance_id, "WebRenderer: explicit unregister");
    registry.schedule_unregister(&instance_id);
    StatusCode::NO_CONTENT
}
