use std::sync::{Arc, Mutex};

use crate::errors::ControlPointError;
use crate::model::PlaybackState;
use crate::music_renderer::RendererFromMediaRendererInfo;
use crate::music_renderer::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackStatus, QueueTransportControl, RendererBackend,
    TransportControl, VolumeControl,
};
use crate::music_renderer::musicrenderer::{MusicRendererBackend, build_didl_lite_metadata};
use crate::queue::{EnqueueMode, MusicQueue, PlaybackItem, QueueBackend, QueueSnapshot};
use crate::upnp_clients::{
    AvTransportClient, ConnectionInfo, ConnectionManagerClient, PositionInfo, ProtocolInfo,
    RenderingControlClient,
};
use crate::{DeviceIdentity, RendererInfo};

/// High-level handle representing a renderer and its optional AVTransport client.
#[derive(Clone, Debug)]
pub struct UpnpRenderer {
    avtransport: Option<AvTransportClient>,
    rendering_control: Option<RenderingControlClient>,
    connection_manager: Option<ConnectionManagerClient>,
    has_avtransport_set_next: bool,
    queue: Arc<Mutex<MusicQueue>>,
}

impl UpnpRenderer {
    /// Retourne true si le renderer à un service de type AVTransport.
    pub fn has_avtransport(&self) -> bool {
        self.avtransport.is_some()
    }

    /// Retourne true si le renderer à un service de type RenderingControl.
    pub fn has_rendering_control(&self) -> bool {
        self.rendering_control.is_some()
    }

    /// Retourne true si le renderer à un service de type ConnectionManager.
    pub fn has_connection_manager(&self) -> bool {
        self.connection_manager.is_some()
    }

    /// Retourne le client de type AVTransport contenant les URL de controle et d'abonnement.
    pub fn avtransport(&self) -> Result<&AvTransportClient, ControlPointError> {
        self.avtransport.as_ref().ok_or_else(|| {
            ControlPointError::upnp_operation_not_supported("AvTransport", "Renderer")
        })
    }

    /// Retourne le client de type RenderingControl contenant les URL de controle et d'abonnement.
    pub fn rendering_control(&self) -> Result<&RenderingControlClient, ControlPointError> {
        self.rendering_control.as_ref().ok_or_else(|| {
            ControlPointError::upnp_operation_not_supported("RenderingControl", "Renderer")
        })
    }

    /// Retourne le client de type ConnectionManager contenant les URL de controle et d'abonnement.
    pub fn connection_manager(&self) -> Result<&ConnectionManagerClient, ControlPointError> {
        self.connection_manager.as_ref().ok_or_else(|| {
            ControlPointError::upnp_operation_not_supported("ConnectionManager", "Renderer")
        })
    }

    pub fn protocol_info(&self) -> Result<ProtocolInfo, ControlPointError> {
        let cm = self.connection_manager()?;
        cm.get_protocol_info()
    }

    pub fn connection_ids(&self) -> Result<Vec<i32>, ControlPointError> {
        let cm = self.connection_manager()?;
        cm.get_current_connection_ids()
    }

    pub fn connection_info(&self, connection_id: i32) -> Result<ConnectionInfo, ControlPointError> {
        let cm = self.connection_manager()?;
        cm.get_current_connection_info(connection_id)
    }

    pub fn set_next_uri(&self, uri: &str, meta: &str) -> Result<(), ControlPointError> {
        if !self.has_avtransport_set_next {
            return Err(ControlPointError::upnp_operation_not_supported(
                "SetNextAVTransportURI",
                "Renderer",
            ));
        }
        let avt = self.avtransport()?;
        avt.set_next_av_transport_uri(uri, meta)
    }
}

impl UpnpRenderer {
    pub fn new(
        avtransport: Option<AvTransportClient>,
        rendering_control: Option<RenderingControlClient>,
        connection_manager: Option<ConnectionManagerClient>,
        has_avtransport_set_next: bool,
        queue: Arc<Mutex<MusicQueue>>,
    ) -> Self {
        Self {
            avtransport,
            rendering_control,
            connection_manager,
            has_avtransport_set_next,
            queue,
        }
    }
}

impl RendererFromMediaRendererInfo for UpnpRenderer {
    fn from_renderer_info(info: &RendererInfo) -> Result<Self, ControlPointError> {
        // Prepare le service AVTTransport
        let avtransport = match (
            info.avtransport_control_url(),
            info.avtransport_service_type(),
        ) {
            (Some(url), Some(service)) => Some(AvTransportClient::new(url, service)),
            _ => None,
        };

        // Prepare le service RenderingControl
        let rendering_control = match (
            info.rendering_control_control_url(),
            info.rendering_control_service_type(),
        ) {
            (Some(url), Some(service)) => Some(RenderingControlClient::new(url, service)),
            _ => None,
        };

        // Prepare le service ConnectionManager
        let connection_manager = match (
            info.connection_manager_control_url(),
            info.connection_manager_service_type(),
        ) {
            (Some(url), Some(service)) => Some(ConnectionManagerClient::new(url, service)),
            _ => None,
        };

        // Exige AVTTransport et RenderingControl au minimum
        if avtransport.is_none() || rendering_control.is_none() {
            return Err(ControlPointError::UpnpError(format!(
                "Some mandatory services are missing on {:?}",
                info.id(),
            )));
        }

        // Create the internal queue
        let queue = Arc::new(Mutex::new(MusicQueue::from_renderer_info(info)?));

        Ok(Self {
            avtransport,
            rendering_control,
            connection_manager,
            has_avtransport_set_next: info.capabilities().has_avtransport_set_next(),
            queue,
        })
    }

    fn to_backend(self) -> MusicRendererBackend {
        MusicRendererBackend::Upnp(self)
    }
}

impl RendererBackend for UpnpRenderer {
    fn queue(&self) -> &Arc<Mutex<MusicQueue>> {
        &self.queue
    }
}

impl QueueTransportControl for UpnpRenderer {
    fn play_from_queue(&self) -> Result<(), ControlPointError> {
        let mut queue = self.queue.lock().unwrap();

        // Get or initialize current index
        let current_index = match queue.current_index()? {
            Some(idx) => idx,
            None => {
                if queue.len()? > 0 {
                    queue.set_index(Some(0))?;
                    0
                } else {
                    return Err(ControlPointError::QueueError("Queue is empty".into()));
                }
            }
        };

        // Get the item
        let item = queue
            .get_item(current_index)?
            .ok_or_else(|| ControlPointError::QueueError("Current item not found".into()))?;

        drop(queue);

        // Build metadata - handle optional TrackMetadata
        let metadata = if let Some(ref track_metadata) = item.metadata {
            build_didl_lite_metadata(track_metadata, &item.uri, &item.protocol_info)
        } else {
            // Fallback to minimal DIDL-Lite if no metadata
            format!(
                r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/"><item id="0" parentID="-1" restricted="1"><res protocolInfo="{}">{}</res></item></DIDL-Lite>"#,
                item.protocol_info, item.uri
            )
        };

        // UPNP: SetAVTransportURI + Play
        let avt = self.avtransport()?;
        avt.set_av_transport_uri(&item.uri, &metadata)?;
        avt.play(0, "1")?;

        Ok(())
    }

    fn play_next(&self) -> Result<(), ControlPointError> {
        {
            let mut queue = self.queue.lock().unwrap();
            if !queue.advance()? {
                return Err(ControlPointError::QueueError("No next track".into()));
            }
        }

        self.play_from_queue()
    }

    fn play_previous(&self) -> Result<(), ControlPointError> {
        {
            let mut queue = self.queue.lock().unwrap();
            if !queue.rewind()? {
                return Err(ControlPointError::QueueError("No previous track".into()));
            }
        }

        self.play_from_queue()
    }

    fn play_from_index(&self, index: usize) -> Result<(), ControlPointError> {
        {
            let mut queue = self.queue.lock().unwrap();
            queue.set_index(Some(index))?;
        }

        self.play_from_queue()
    }
}

impl QueueBackend for UpnpRenderer {
    fn len(&self) -> Result<usize, ControlPointError> {
        self.queue.lock().unwrap().len()
    }

    fn track_ids(&self) -> Result<Vec<u32>, ControlPointError> {
        self.queue.lock().unwrap().track_ids()
    }

    fn id_to_position(&self, id: u32) -> Result<usize, ControlPointError> {
        self.queue.lock().unwrap().id_to_position(id)
    }

    fn position_to_id(&self, id: usize) -> Result<u32, ControlPointError> {
        self.queue.lock().unwrap().position_to_id(id)
    }

    fn current_track(&self) -> Result<Option<u32>, ControlPointError> {
        self.queue.lock().unwrap().current_track()
    }

    fn current_index(&self) -> Result<Option<usize>, ControlPointError> {
        self.queue.lock().unwrap().current_index()
    }

    fn queue_snapshot(&self) -> Result<QueueSnapshot, ControlPointError> {
        self.queue.lock().unwrap().queue_snapshot()
    }

    fn set_index(&mut self, index: Option<usize>) -> Result<(), ControlPointError> {
        self.queue.lock().unwrap().set_index(index)
    }

    fn replace_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<(), ControlPointError> {
        self.queue
            .lock()
            .unwrap()
            .replace_queue(items, current_index)
    }

    fn sync_queue(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        self.queue.lock().unwrap().sync_queue(items)
    }

    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>, ControlPointError> {
        self.queue.lock().unwrap().get_item(index)
    }

    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<(), ControlPointError> {
        self.queue.lock().unwrap().replace_item(index, item)
    }

    fn enqueue_items(
        &mut self,
        items: Vec<PlaybackItem>,
        mode: EnqueueMode,
    ) -> Result<(), ControlPointError> {
        self.queue.lock().unwrap().enqueue_items(items, mode)
    }
}

/// Implémentation UPnP AV de `TransportControl` pour [`UpnpRenderer`].
///
/// Cette impl se base sur AVTransport (InstanceID = 0).
impl TransportControl for UpnpRenderer {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<(), ControlPointError> {
        let avt = self.avtransport()?;
        avt.set_av_transport_uri(uri, meta)?;
        avt.play(0, "1")
    }

    fn play(&self) -> Result<(), ControlPointError> {
        let avt = self.avtransport()?;
        avt.play(0, "1")
    }

    fn pause(&self) -> Result<(), ControlPointError> {
        let avt = self.avtransport()?;
        avt.pause(0)
    }

    fn stop(&self) -> Result<(), ControlPointError> {
        let avt = self.avtransport()?;
        avt.stop(0)
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<(), ControlPointError> {
        let avt = self.avtransport()?;
        avt.seek(0, "REL_TIME", hhmmss)
    }
}

/// Implémentation UPnP RenderingControl de `VolumeControl` pour [`UpnpRenderer`].
///
/// Cette impl se base sur le channel "Master" (InstanceID = 0).
impl VolumeControl for UpnpRenderer {
    fn volume(&self) -> Result<u16, ControlPointError> {
        let rc = self.rendering_control()?;
        rc.get_volume(0, "Master")
    }

    fn set_volume(&self, v: u16) -> Result<(), ControlPointError> {
        let rc = self.rendering_control()?;
        rc.set_volume(0, "Master", v)
    }

    fn mute(&self) -> Result<bool, ControlPointError> {
        let rc = self.rendering_control()?;
        rc.get_mute(0, "Master")
    }

    fn set_mute(&self, m: bool) -> Result<(), ControlPointError> {
        let rc = self.rendering_control()?;
        rc.set_mute(0, "Master", m)
    }
}

/// Implémentation UPnP AV de `PlaybackStatus` pour [`UpnpRenderer`].
///
/// Utilise AVTransport::GetTransportInfo(InstanceID=0).
impl PlaybackStatus for UpnpRenderer {
    fn playback_state(&self) -> Result<PlaybackState, ControlPointError> {
        let avt = self.avtransport()?;
        let info = avt.get_transport_info(0)?;
        Ok(PlaybackState::from_upnp_state(
            &info.current_transport_state,
        ))
    }
}

impl PlaybackPosition for UpnpRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo, ControlPointError> {
        let avt = self.avtransport()?;
        let raw: PositionInfo = avt.get_position_info(0)?;

        Ok(PlaybackPositionInfo {
            track: Some(raw.track),
            rel_time: raw.rel_time,
            abs_time: raw.abs_time,
            track_duration: raw.track_duration,
            track_metadata: raw.track_metadata,
            track_uri: raw.track_uri,
        })
    }
}
