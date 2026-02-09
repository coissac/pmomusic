//! Gestionnaire de sessions WebRenderer

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;

use pmoupnp::devices::DeviceInstance;

use crate::messages::ServerMessage;
use crate::state::SharedState;

/// Session WebSocket liée à un MediaRenderer privé
pub struct WebRendererSession {
    pub token: String,
    pub udn: String,
    pub device_instance: Arc<DeviceInstance>,
    pub ws_sender: mpsc::UnboundedSender<ServerMessage>,
    pub state: SharedState,
    pub created_at: SystemTime,
    pub last_activity: Arc<RwLock<SystemTime>>,
}

/// Gestionnaire global des sessions
#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Arc<WebRendererSession>>>>,
    timeout_duration: Duration,
}

impl SessionManager {
    pub fn new(timeout_duration: Duration) -> Self {
        let manager = Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            timeout_duration,
        };

        manager.spawn_cleanup_task();
        manager
    }

    pub fn add_session(&self, session: Arc<WebRendererSession>) {
        let token = session.token.clone();
        self.sessions.write().insert(token.clone(), session);
        tracing::info!(token = %token, "WebRenderer session added");
    }

    pub fn get_session(&self, token: &str) -> Option<Arc<WebRendererSession>> {
        let sessions = self.sessions.read();
        if let Some(session) = sessions.get(token) {
            *session.last_activity.write() = SystemTime::now();
            Some(session.clone())
        } else {
            None
        }
    }

    pub fn remove_session(&self, token: &str) -> Option<Arc<WebRendererSession>> {
        let session = self.sessions.write().remove(token);
        if let Some(ref s) = session {
            tracing::info!(token = %s.token, udn = %s.udn, "WebRenderer session removed");
        }
        session
    }

    pub fn list_sessions(&self) -> Vec<Arc<WebRendererSession>> {
        self.sessions.read().values().cloned().collect()
    }

    fn spawn_cleanup_task(&self) {
        let sessions = self.sessions.clone();
        let timeout = self.timeout_duration;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;

                let now = SystemTime::now();
                let mut to_remove = Vec::new();

                {
                    let sessions_guard = sessions.read();
                    for (token, session) in sessions_guard.iter() {
                        let last = session.last_activity.read();
                        if let Ok(elapsed) = now.duration_since(*last) {
                            if elapsed > timeout {
                                to_remove.push(token.clone());
                            }
                        }
                    }
                }

                if !to_remove.is_empty() {
                    let mut sessions_guard = sessions.write();
                    for token in to_remove {
                        sessions_guard.remove(&token);
                        tracing::info!(token = %token, "Session expired and removed");
                    }
                }
            }
        });
    }
}
