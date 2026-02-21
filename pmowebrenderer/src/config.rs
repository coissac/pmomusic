//! Intégration avec pmoserver — enregistrement des routes WebRenderer

#[cfg(feature = "pmoserver")]
use std::sync::Arc;
#[cfg(feature = "pmoserver")]
use std::time::Duration;

#[cfg(feature = "pmoserver")]
use async_trait::async_trait;

#[cfg(feature = "pmoserver")]
use pmocontrol::ControlPoint;

#[cfg(feature = "pmoserver")]
use crate::error::WebRendererError;
#[cfg(feature = "pmoserver")]
use crate::session::SessionManager;
#[cfg(feature = "pmoserver")]
use crate::websocket::{websocket_handler, WebSocketState};

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
        let session_manager = Arc::new(SessionManager::new(Duration::from_secs(30 * 60)));

        let ws_state = WebSocketState {
            session_manager,
            control_point,
        };

        self.add_any_handler_with_state("/api/webrenderer/ws", websocket_handler, ws_state)
            .await;

        tracing::info!("WebRenderer WebSocket endpoint registered at /api/webrenderer/ws");
        Ok(())
    }
}

#[cfg(not(feature = "pmoserver"))]
pub trait WebRendererExt {}

#[cfg(not(feature = "pmoserver"))]
impl WebRendererExt for () {}
