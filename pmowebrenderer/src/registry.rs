//! Registre des instances WebRenderer actives.
//!
//! Remplace `SessionManager` et `websocket.rs`. La session est maintenant liée
//! au flux FLAC HTTP, pas à une connexion WebSocket.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use pmoupnp::devices::DeviceInstance;

use crate::error::WebRendererError;
use crate::pipeline::{InstancePipeline, PipelineHandle};
use crate::renderer::WebRendererFactory;
use crate::state::{RendererState, SharedState};
use crate::messages::PlaybackState;

#[cfg(feature = "pmoserver")]
use pmocontrol::{ControlPoint, DeviceId};
#[cfg(feature = "pmoserver")]
use pmocontrol::model::{RendererCapabilities, RendererProtocol};
#[cfg(feature = "pmoserver")]
use pmoupnp::UpnpTypedInstance;

/// Une instance WebRenderer côté serveur
pub struct WebRendererInstance {
    pub instance_id: String,
    pub udn: String,
    pub device_instance: Arc<DeviceInstance>,
    pub state: SharedState,
    /// Handle vers le sink OGG-FLAC — clonable, subscribe() crée un flux indépendant par client.
    pub flac_handle: pmoaudio_ext::sinks::OggFlacStreamHandle,
    pub pipeline: PipelineHandle,
    pub created_at: SystemTime,
}

/// Registre global des instances WebRenderer
pub struct RendererRegistry {
    /// Map instance_id → instance
    instances: RwLock<HashMap<String, Arc<WebRendererInstance>>>,
    /// Map udn → instance (pour retrouver depuis les handlers UPnP)
    by_udn: RwLock<HashMap<String, Arc<WebRendererInstance>>>,
    /// Tokens d'annulation des unregister différés (instance_id → token)
    pending_unregister: RwLock<HashMap<String, tokio_util::sync::CancellationToken>>,
    #[cfg(feature = "pmoserver")]
    control_point: Arc<ControlPoint>,
}

impl RendererRegistry {
    #[cfg(feature = "pmoserver")]
    pub fn new(control_point: Arc<ControlPoint>) -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
            by_udn: RwLock::new(HashMap::new()),
            pending_unregister: RwLock::new(HashMap::new()),
            control_point,
        }
    }

    #[cfg(not(feature = "pmoserver"))]
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
            by_udn: RwLock::new(HashMap::new()),
            pending_unregister: RwLock::new(HashMap::new()),
        }
    }

    /// Enregistre ou reconnecte une instance.
    /// Retourne `(stream_url, udn)`.
    pub async fn register_or_reconnect(
        &self,
        instance_id: &str,
        user_agent: &str,
    ) -> Result<(String, String), WebRendererError> {
        // Annuler tout unregister différé pour cet instance_id
        if let Some(cancel) = self.pending_unregister.write().remove(instance_id) {
            tracing::info!(instance_id = %instance_id, "WebRenderer: cancelled pending unregister (page reload)");
            cancel.cancel();
        }

        // Reconnexion : l'instance existe déjà (ou vient d'être conservée)
        {
            let instances = self.instances.read();
            if let Some(existing) = instances.get(instance_id) {
                tracing::info!(instance_id = %instance_id, "WebRenderer: reconnecting existing instance");
                #[cfg(feature = "pmoserver")]
                self.register_with_control_point(&existing.device_instance)?;
                let stream_url = format!("/api/webrenderer/{}/stream", instance_id);
                return Ok((stream_url, existing.udn.clone()));
            }
        }

        // Première connexion : créer device UPnP + pipeline
        let instance = self.create_instance(instance_id, user_agent).await?;
        let instance = Arc::new(instance);
        let stream_url = format!("/api/webrenderer/{}/stream", instance_id);
        let udn = instance.udn.clone();

        {
            let mut instances = self.instances.write();
            instances.insert(instance_id.to_string(), instance.clone());
        }
        {
            let mut by_udn = self.by_udn.write();
            by_udn.insert(instance.udn.clone(), instance.clone());
        }

        tracing::info!(
            instance_id = %instance_id,
            udn = %udn,
            "WebRenderer: new instance registered"
        );

        Ok((stream_url, udn))
    }

    /// Retourne un OggFlacClientStream indépendant pour l'endpoint /stream.
    /// Chaque appel crée un nouveau subscriber broadcast — safe pour connexions simultanées.
    pub fn get_stream(
        &self,
        instance_id: &str,
    ) -> Option<pmoaudio_ext::sinks::OggFlacClientStream> {
        self.instances
            .read()
            .get(instance_id)
            .map(|i| i.flac_handle.subscribe())
    }

    /// Retourne le PipelineHandle par UDN (pour les handlers UPnP)
    pub fn get_pipeline_by_udn(&self, udn: &str) -> Option<PipelineHandle> {
        self.by_udn
            .read()
            .get(udn)
            .map(|i| i.pipeline.clone())
    }

    /// Retourne le SharedState par UDN
    pub fn get_state_by_udn(&self, udn: &str) -> Option<SharedState> {
        self.by_udn
            .read()
            .get(udn)
            .map(|i| i.state.clone())
    }

    /// Retourne le DeviceInstance par UDN
    pub fn get_device_by_udn(&self, udn: &str) -> Option<Arc<DeviceInstance>> {
        self.by_udn
            .read()
            .get(udn)
            .map(|i| i.device_instance.clone())
    }

    /// Met à jour la position et la durée depuis le navigateur (audio.currentTime).
    pub fn update_position(&self, instance_id: &str, position_sec: f64, duration_sec: Option<f64>) {
        let instances = self.instances.read();
        if let Some(instance) = instances.get(instance_id) {
            let mut s = instance.state.write();
            s.position = Some(crate::pipeline::seconds_to_upnp_time(position_sec));
            if let Some(dur) = duration_sec {
                if dur > 0.0 {
                    s.duration = Some(crate::pipeline::seconds_to_upnp_time(dur));
                }
            }
        }
    }

    pub fn schedule_unregister(self: &Arc<Self>, instance_id: &str) {
        use tokio_util::sync::CancellationToken;

        // Ne pas détruire immédiatement : attendre 5s au cas où la page se recharge
        let cancel = CancellationToken::new();
        self.pending_unregister.write().insert(instance_id.to_string(), cancel.clone());

        let instance_id_owned = instance_id.to_string();
        let registry = Arc::clone(self);

        tracing::info!(instance_id = %instance_id, "WebRenderer: unregister scheduled (5s grace period)");

        tokio::spawn(async move {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!(instance_id = %instance_id_owned, "WebRenderer: deferred unregister cancelled (page reload)");
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                    registry.pending_unregister.write().remove(&instance_id_owned);
                    let instance = registry.instances.write().remove(&instance_id_owned);
                    if let Some(instance) = instance {
                        registry.by_udn.write().remove(&instance.udn);
                        instance.pipeline.stop_token.cancel();
                        #[cfg(feature = "pmoserver")]
                        if let Ok(mut reg) = registry.control_point.registry().write() {
                            reg.device_says_byebye(&instance.udn);
                        }
                        tracing::info!(
                            instance_id = %instance_id_owned,
                            udn = %instance.udn,
                            "WebRenderer: instance unregistered"
                        );
                    }
                }
            }
        });
    }

    // ── Création d'instance ────────────────────────────────────────────────────

    async fn create_instance(
        &self,
        instance_id: &str,
        user_agent: &str,
    ) -> Result<WebRendererInstance, WebRendererError> {
        // UDN stable dérivé de l'instance_id
        let candidate_udn = instance_id.to_ascii_lowercase();
        let full_udn = format!("uuid:{}", candidate_udn);

        // Persister l'UDN dans la config (pour que device_instance.rs le retrouve)
        if let Err(e) = pmoconfig::get_config().set_device_udn(
            "MediaRenderer",
            instance_id,
            candidate_udn.clone(),
        ) {
            tracing::warn!("WebRenderer: failed to persist UDN: {:?}", e);
        }

        let state: SharedState = Arc::new(parking_lot::RwLock::new(RendererState::default()));

        #[cfg(feature = "pmoserver")]
        let (device_instance, pipeline) = {
            use pmoupnp::UpnpServerExt;

            let server_arc = pmoserver::get_server()
                .ok_or(WebRendererError::ServerNotAvailable)?;

            // Créer le pipeline d'abord pour avoir le PipelineHandle
            let ip = InstancePipeline::start(
                state.clone(),
                self.control_point.clone(),
                full_udn.clone(),
            );
            let pipeline = ip.pipeline_handle.clone();

            // Vérifier si un device avec ce même UDN existe déjà
            let existing_di = {
                let server = server_arc.read().await;
                server.get_device(&candidate_udn)
            };

            let di = if let Some(di) = existing_di {
                tracing::info!(udn = %candidate_udn, "WebRenderer: reusing device from registry");
                di
            } else {
                // Créer le device UPnP
                tracing::info!(udn = %candidate_udn, "WebRenderer: creating new device");
                let device = WebRendererFactory::create_device_with_pipeline(
                    instance_id,
                    user_agent,
                    pipeline.clone(),
                    state.clone(),
                )
                .map_err(|e| WebRendererError::DeviceCreationError(e.to_string()))?;

                let mut server = server_arc.write().await;
                server
                    .register_device(Arc::new(device), false)
                    .await
                    .map_err(|e| WebRendererError::RegistrationError(e.to_string()))?
            };

            self.register_with_control_point(&di)?;
            (di, ip)
        };

        #[cfg(not(feature = "pmoserver"))]
        let (device_instance, pipeline) = {
            use pmoupnp::UpnpModel;

            let ip = InstancePipeline::start(state.clone(), full_udn.clone());
            let pipeline = ip.pipeline_handle.clone();

            let device = WebRendererFactory::create_device_with_pipeline(
                instance_id,
                user_agent,
                pipeline.clone(),
                state.clone(),
            )
            .map_err(|e| WebRendererError::DeviceCreationError(e.to_string()))?;

            (Arc::new(device).create_instance(), ip)
        };

        Ok(WebRendererInstance {
            instance_id: instance_id.to_string(),
            udn: full_udn,
            device_instance,
            state,
            flac_handle: pipeline.flac_handle.clone(),
            pipeline: pipeline.pipeline_handle,
            created_at: SystemTime::now(),
        })
    }

    /// Enregistre le device dans le ControlPoint
    #[cfg(feature = "pmoserver")]
    fn register_with_control_point(
        &self,
        di: &Arc<DeviceInstance>,
    ) -> Result<(), WebRendererError> {
        let base_url = di.base_url().to_string();
        let udn = di.udn().to_ascii_lowercase();
        let udn_with_prefix = format!("uuid:{}", udn);
        let device_route = di.route();
        let model = di.get_model();

        let avtransport_control_url = Some(format!(
            "{}{}/service/AVTransport/control",
            base_url, device_route
        ));
        let rendering_control_url = Some(format!(
            "{}{}/service/RenderingControl/control",
            base_url, device_route
        ));
        let connection_manager_url = Some(format!(
            "{}{}/service/ConnectionManager/control",
            base_url, device_route
        ));

        let renderer_info = pmocontrol::RendererInfo::make(
            DeviceId(udn_with_prefix.clone()),
            udn_with_prefix.clone(),
            model.friendly_name().to_string(),
            model.model_name().to_string(),
            "PMOMusic".to_string(),
            RendererProtocol::UpnpAvOnly,
            RendererCapabilities {
                has_avtransport: true,
                has_avtransport_set_next: true,
                has_rendering_control: true,
                has_connection_manager: true,
                ..Default::default()
            },
            format!("{}{}", base_url, di.description_route()),
            "PMOMusic WebRenderer/2.0".to_string(),
            Some("urn:schemas-upnp-org:service:AVTransport:1".to_string()),
            avtransport_control_url,
            Some("urn:schemas-upnp-org:service:RenderingControl:1".to_string()),
            rendering_control_url,
            Some("urn:schemas-upnp-org:service:ConnectionManager:1".to_string()),
            connection_manager_url,
            None, None, None, None, None, None, None, None, None, None, None, None, None, None, None,
        );

        if let Ok(mut registry) = self.control_point.registry().write() {
            registry.push_renderer(&renderer_info, 86400);
        }

        tracing::info!(udn = %udn, "WebRenderer: registered with ControlPoint");
        Ok(())
    }
}
