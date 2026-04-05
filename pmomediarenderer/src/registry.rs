//! Registre des instances MediaRenderer actives.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use pmoupnp::devices::DeviceInstance;

use crate::error::MediaRendererError;
use crate::pipeline::{InstancePipeline, PipelineHandle};
use crate::renderer::MediaRendererFactory;
use crate::state::{RendererState, SharedState};
use super::adapter::DeviceAdapter;


#[cfg(feature = "pmoserver")]
use pmocontrol::{ControlPoint, DeviceId};
#[cfg(feature = "pmoserver")]
use pmocontrol::model::{RendererCapabilities, RendererProtocol};
#[cfg(feature = "pmoserver")]
use pmoupnp::UpnpTypedInstance;

pub struct MediaRendererInstance {
    pub instance_id: String,
    pub udn: String,
    pub device_instance: Arc<DeviceInstance>,
    pub state: SharedState,
    pub flac_handle: pmoaudio_ext::sinks::OggFlacStreamHandle,
    pub pipeline: PipelineHandle,
    pub created_at: SystemTime,
    pub adapter: Arc<dyn DeviceAdapter>,
}

pub struct MediaRendererRegistry {
    instances: RwLock<HashMap<String, Arc<MediaRendererInstance>>>,
    by_udn: RwLock<HashMap<String, Arc<MediaRendererInstance>>>,
    pending_unregister: RwLock<HashMap<String, tokio_util::sync::CancellationToken>>,
    #[cfg(feature = "pmoserver")]
    control_point: Arc<ControlPoint>,
}

impl MediaRendererRegistry {
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

    pub async fn register_or_reconnect(
        &self,
        instance_id: &str,
        stream_url_base: &str,
        renderer_name: &str,
        friendly_name: &str,
        adapter_fn: impl FnOnce(SharedState) -> Arc<dyn DeviceAdapter>,
    ) -> Result<(String, String, bool), MediaRendererError> {
        if let Some(cancel) = self.pending_unregister.write().remove(instance_id) {
            tracing::info!(instance_id = %instance_id, "MediaRenderer: cancelled pending unregister (page reload)");
            cancel.cancel();
        }

        {
            let instances = self.instances.read();
            if let Some(existing) = instances.get(instance_id) {
                tracing::info!(instance_id = %instance_id, "MediaRenderer: reconnecting existing instance");
                #[cfg(feature = "pmoserver")]
                self.register_with_control_point(&existing.device_instance, renderer_name, &existing.udn)?;
                let stream_url = format!("{}/{}/stream", stream_url_base, instance_id);
                let should_play = {
                    let s = existing.state.read();
                    s.current_uri.is_some() && matches!(
                        s.playback_state,
                        crate::messages::PlaybackState::Playing | crate::messages::PlaybackState::Transitioning
                    )
                };
                return Ok((stream_url, existing.udn.clone(), should_play));
            }
        }

        let instance = self.create_instance_with_adapter(instance_id, stream_url_base, renderer_name, friendly_name, adapter_fn).await?;
        let instance = Arc::new(instance);
        let stream_url = format!("{}/{}/stream", stream_url_base, instance_id);
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
            "MediaRenderer: new instance registered"
        );

        Ok((stream_url, udn, false))
    }

    pub fn get_stream(
        &self,
        instance_id: &str,
    ) -> Option<pmoaudio_ext::sinks::OggFlacClientStream> {
        let instances = self.instances.read();
        match instances.get(instance_id) {
            Some(i) => {
                tracing::debug!(instance_id = %instance_id, "Found instance, getting flac_handle");
                Some(i.flac_handle.subscribe())
            }
            None => {
                tracing::error!(instance_id = %instance_id, "Instance not found in registry!");
                None
            }
        }
    }

    pub fn get_pipeline_by_udn(&self, udn: &str) -> Option<PipelineHandle> {
        self.by_udn
            .read()
            .get(udn)
            .map(|i| i.pipeline.clone())
    }

    pub fn get_instance(&self, instance_id: &str) -> Option<Arc<MediaRendererInstance>> {
        self.instances.read().get(instance_id).cloned()
    }

    pub fn get_state(&self, instance_id: &str) -> Option<SharedState> {
        self.instances
            .read()
            .get(instance_id)
            .map(|i| i.state.clone())
    }

    pub fn get_pipeline(&self, instance_id: &str) -> Option<PipelineHandle> {
        self.instances.read().get(instance_id).map(|i| i.pipeline.clone())
    }

    pub fn get_state_and_udn(&self, instance_id: &str) -> Option<(SharedState, String)> {
        self.instances
            .read()
            .get(instance_id)
            .map(|i| (i.state.clone(), i.udn.clone()))
    }

    pub fn get_state_by_udn(&self, udn: &str) -> Option<SharedState> {
        self.by_udn
            .read()
            .get(udn)
            .map(|i| i.state.clone())
    }

    pub fn get_device_by_udn(&self, udn: &str) -> Option<Arc<DeviceInstance>> {
        self.by_udn
            .read()
            .get(udn)
            .map(|i| i.device_instance.clone())
    }

    pub fn update_duration(&self, instance_id: &str, duration_sec: Option<f64>) {
        let instances = self.instances.read();
        if let Some(instance) = instances.get(instance_id) {
            let mut s = instance.state.write();
            if s.duration.is_none() {
                if let Some(dur) = duration_sec {
                    if dur > 0.0 {
                        s.duration = Some(crate::pipeline::seconds_to_upnp_time(dur));
                    }
                }
            }
        }
    }

    pub fn schedule_unregister(self: &Arc<Self>, instance_id: &str) {
        use tokio_util::sync::CancellationToken;

        let cancel = CancellationToken::new();
        self.pending_unregister.write().insert(instance_id.to_string(), cancel.clone());

        let instance_id_owned = instance_id.to_string();
        let registry = Arc::clone(self);

        tracing::info!(instance_id = %instance_id, "MediaRenderer: unregister scheduled (5s grace period)");

        tokio::spawn(async move {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!(instance_id = %instance_id_owned, "MediaRenderer: deferred unregister cancelled (page reload)");
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
                            "MediaRenderer: instance unregistered"
                        );
                    }
                }
            }
        });
    }

    /// Créer une nouvelle instance avec un adapter fourni via une factory closure.
    /// La closure reçoit le SharedState de l'instance afin que l'adapter partage le même état.
    pub async fn create_instance_with_adapter(
        &self,
        instance_id: &str,
        stream_url_base: &str,
        renderer_name: &str,
        friendly_name: &str,
        adapter_fn: impl FnOnce(SharedState) -> Arc<dyn DeviceAdapter>,
    ) -> Result<MediaRendererInstance, MediaRendererError> {
        let candidate_udn = instance_id.to_ascii_lowercase();
        let full_udn = format!("uuid:{}", candidate_udn);

        // Note: WebRenderers don't need to persist UDN to config since they're tied to browser tabs
        // (instance_id comes from sessionStorage, not a persistent device)

        let state: SharedState = Arc::new(parking_lot::RwLock::new(RendererState::default()));
        let adapter = adapter_fn(state.clone());

        #[cfg(feature = "pmoserver")]
        let (device_instance, pipeline) = {
            use pmoupnp::UpnpServerExt;

            let server_arc = pmoserver::get_server()
                .ok_or(MediaRendererError::ServerNotAvailable)?;

            let ip = InstancePipeline::start(
                state.clone(),
                self.control_point.clone(),
                full_udn.clone(),
                adapter.clone(),
            );
            let pipeline = ip.pipeline_handle.clone();

            let existing_di = {
                let server = server_arc.read().await;
                server.get_device(&candidate_udn)
            };

            let di = if let Some(di) = existing_di {
                tracing::info!(udn = %candidate_udn, "MediaRenderer: reusing device from registry");
                di
            } else {
                tracing::info!(udn = %candidate_udn, "MediaRenderer: creating new device");
                let device = MediaRendererFactory::create_device_with_pipeline(
                    instance_id,
                    "MediaRenderer",
                    friendly_name,
                    pipeline.clone(),
                    state.clone(),
                    stream_url_base,
                )
                .map_err(|e| MediaRendererError::DeviceCreationError(e.to_string()))?;

                let mut server = server_arc.write().await;
                server
                    .register_device(Arc::new(device), false)
                    .await
                    .map_err(|e| MediaRendererError::RegistrationError(e.to_string()))?
            };

            self.register_with_control_point(&di, renderer_name, &full_udn)?;
            (di, ip)
        };

        #[cfg(not(feature = "pmoserver"))]
        let (device_instance, pipeline) = {
            use pmoupnp::UpnpModel;

            let ip = InstancePipeline::start(
                state.clone(),
                full_udn.clone(),
                adapter.clone(),
            );
            let pipeline = ip.pipeline_handle.clone();

            let device = MediaRendererFactory::create_device_with_pipeline(
                instance_id,
                "MediaRenderer",
                friendly_name,
                pipeline.clone(),
                state.clone(),
                stream_url_base,
            )
            .map_err(|e| MediaRendererError::DeviceCreationError(e.to_string()))?;

            (Arc::new(device).create_instance(), ip)
        };

        Ok(MediaRendererInstance {
            instance_id: instance_id.to_string(),
            udn: full_udn,
            device_instance,
            state,
            flac_handle: pipeline.flac_handle.clone(),
            pipeline: pipeline.pipeline_handle,
            created_at: SystemTime::now(),
            adapter,
        })
    }

    #[cfg(feature = "pmoserver")]
    fn register_with_control_point(
        &self,
        di: &Arc<DeviceInstance>,
        renderer_name: &str,
        instance_udn: &str,
    ) -> Result<(), MediaRendererError> {
        let base_url = di.base_url().to_string();
        // Use the instance's UDN, not the device's stored UDN
        let udn = instance_udn.trim_start_matches("uuid:").to_ascii_lowercase();
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
            "PMOMusic-WebRenderer".to_string(),
            RendererProtocol::UpnpAvOnly,
            RendererCapabilities {
                has_avtransport: true,
                has_avtransport_set_next: true,
                has_rendering_control: true,
                has_connection_manager: true,
                ..Default::default()
            },
            format!("{}{}", base_url, di.description_route()),
            renderer_name.to_string(),
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

        tracing::info!(udn = %udn, "MediaRenderer: registered with ControlPoint");
        Ok(())
    }
}