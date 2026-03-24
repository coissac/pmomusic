//! Intégration avec pmoserver — enregistrement des routes WebRenderer

#[cfg(feature = "pmoserver")]
use std::sync::Arc;

#[cfg(feature = "pmoserver")]
use async_trait::async_trait;

#[cfg(feature = "pmoserver")]
use axum::{Router, routing::{delete, get, post}};

#[cfg(feature = "pmoserver")]
use pmocontrol::ControlPoint;

#[cfg(feature = "pmoserver")]
use crate::error::WebRendererError;
#[cfg(feature = "pmoserver")]
use crate::register::{position_update_handler, register_handler, unregister_handler};
#[cfg(feature = "pmoserver")]
use crate::registry::RendererRegistry;
#[cfg(feature = "pmoserver")]
use crate::stream::stream_handler;

/// Trait pour étendre pmoserver::Server avec les routes WebRenderer
#[cfg(feature = "pmoserver")]
#[async_trait]
pub trait WebRendererExt {
    async fn register_web_renderer(
        &mut self,
        control_point: Arc<ControlPoint>,
    ) -> Result<(), WebRendererError>;
}

#[cfg(feature = "pmoserver")]
#[async_trait]
impl WebRendererExt for pmoserver::Server {
    async fn register_web_renderer(
        &mut self,
        control_point: Arc<ControlPoint>,
    ) -> Result<(), WebRendererError> {
        let registry = Arc::new(RendererRegistry::new(control_point));

        // POST /api/webrenderer/register
        self.add_post_handler_with_state(
            "/api/webrenderer/register",
            register_handler,
            registry.clone(),
        )
        .await;

        // GET /api/webrenderer/{id}/stream  +  DELETE /api/webrenderer/{id}  +  POST /api/webrenderer/{id}/position
        let dynamic_router = Router::new()
            .route("/{id}/stream", get(stream_handler))
            .route("/{id}/position", post(position_update_handler))
            .route("/{id}", delete(unregister_handler))
            .with_state(registry.clone());
        self.add_router("/api/webrenderer", dynamic_router).await;

        tracing::info!("WebRenderer server-side streaming endpoints registered");
        tracing::info!("  POST   /api/webrenderer/register");
        tracing::info!("  GET    /api/webrenderer/{{id}}/stream");
        tracing::info!("  POST   /api/webrenderer/{{id}}/position");
        tracing::info!("  DELETE /api/webrenderer/{{id}}");
        Ok(())
    }
}

#[cfg(not(feature = "pmoserver"))]
pub trait WebRendererExt {}

#[cfg(not(feature = "pmoserver"))]
impl WebRendererExt for () {}
