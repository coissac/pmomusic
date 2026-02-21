//! Gestionnaire de sessions WebRenderer

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use pmoupnp::devices::DeviceInstance;

use crate::state::{SharedSender, SharedState};

/// Session WebSocket liée à un MediaRenderer privé
pub struct WebRendererSession {
    pub token: String,
    pub udn: String,
    pub device_instance: Arc<DeviceInstance>,
    /// Sender partagé : mis à jour à chaque reconnexion WebSocket.
    pub shared_sender: SharedSender,
    pub state: SharedState,
    pub created_at: SystemTime,
    pub last_activity: Arc<RwLock<SystemTime>>,
}

/// Gestionnaire global des sessions
#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Arc<WebRendererSession>>>>,
    /// Map UDN → SharedSender, persiste même après suppression de la session.
    /// Permet de retrouver et mettre à jour le sender à la reconnexion.
    senders: Arc<RwLock<HashMap<String, SharedSender>>>,
    /// Map UDN → SharedState, persiste même après suppression de la session.
    /// Permet de réutiliser l'état partagé avec les handlers du device existant.
    states: Arc<RwLock<HashMap<String, SharedState>>>,
    timeout_duration: Duration,
}

impl SessionManager {
    pub fn new(timeout_duration: Duration) -> Self {
        let manager = Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            senders: Arc::new(RwLock::new(HashMap::new())),
            states: Arc::new(RwLock::new(HashMap::new())),
            timeout_duration,
        };

        manager.spawn_cleanup_task();
        manager
    }

    pub fn add_session(&self, session: Arc<WebRendererSession>) {
        let token = session.token.clone();
        let udn = session.udn.clone();
        let sender = session.shared_sender.clone();
        let state = session.state.clone();
        self.sessions.write().insert(token.clone(), session);
        self.senders.write().insert(udn.clone(), sender);
        self.states.write().insert(udn, state);
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

    /// Retrouve une session par UDN du device (indépendant du token WebSocket).
    pub fn get_session_by_udn(&self, udn: &str) -> Option<Arc<WebRendererSession>> {
        let sessions = self.sessions.read();
        sessions.values().find(|s| s.udn == udn).cloned()
    }

    /// Retrouve le SharedSender par UDN (persiste même après suppression de session).
    pub fn get_sender_by_udn(&self, udn: &str) -> Option<SharedSender> {
        self.senders.read().get(udn).cloned()
    }

    /// Retrouve le SharedState par UDN (persiste même après suppression de session).
    pub fn get_state_by_udn(&self, udn: &str) -> Option<SharedState> {
        self.states.read().get(udn).cloned()
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
