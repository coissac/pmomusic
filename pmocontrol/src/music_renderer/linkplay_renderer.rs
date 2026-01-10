use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ureq::Agent;

use crate::DeviceIdentity;
use crate::errors::ControlPointError;
use crate::linkplay_client::{
    LinkPlayStatus, build_agent, extract_linkplay_host, fetch_status_for_host, percent_encode,
};
use crate::model::{PlaybackState, RendererInfo};
use crate::music_renderer::RendererFromMediaRendererInfo;
use crate::music_renderer::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackStatus, QueueTransportControl, RendererBackend,
    TransportControl, VolumeControl,
};
use crate::music_renderer::musicrenderer::MusicRendererBackend;
use crate::music_renderer::time_utils::parse_hhmmss_strict;
use crate::queue::MusicQueue;
use crate::queue::{EnqueueMode, PlaybackItem, QueueBackend, QueueSnapshot};

const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 3;

/// Renderer backend for devices exposing the LinkPlay HTTP API.
#[derive(Clone)]
pub struct LinkPlayRenderer {
    host: String,
    timeout: Duration,
    queue: Arc<Mutex<MusicQueue>>,
}

impl fmt::Debug for LinkPlayRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkPlayRenderer")
            .field("host", &self.host)
            .finish()
    }
}

impl LinkPlayRenderer {
    fn agent(&self) -> Agent {
        build_agent(self.timeout)
    }

    fn send_player_command(&self, command: &str) -> Result<(), ControlPointError> {
        let url = format!(
            "http://{}/httpapi.asp?command=setPlayerCmd:{}",
            self.host, command
        );
        self.agent().get(&url).call().map_err(|_| {
            ControlPointError::ArilycTcpError(format!(
                "LinkPlay command {} failed for {}",
                command, self.host
            ))
        })?;
        Ok(())
    }

    fn fetch_status(&self) -> Result<LinkPlayStatus, ControlPointError> {
        fetch_status_for_host(&self.host, self.timeout)
    }
}

impl RendererFromMediaRendererInfo for LinkPlayRenderer {
    fn from_renderer_info(info: &RendererInfo) -> Result<Self, ControlPointError> {
        let host = extract_linkplay_host(&info.location()).ok_or_else(|| {
            ControlPointError::LinkPlayError(format!(
                "Renderer {} has no valid LOCATION host",
                info.udn()
            ))
        })?;

        let queue = Arc::new(Mutex::new(MusicQueue::from_renderer_info(info)?));

        Ok(Self {
            host,
            timeout: Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECS),
            queue,
        })
    }

    fn to_backend(self) -> MusicRendererBackend {
        MusicRendererBackend::LinkPlay(self)
    }
}

impl TransportControl for LinkPlayRenderer {
    fn play_uri(&self, uri: &str, _meta: &str) -> Result<(), ControlPointError> {
        let encoded = percent_encode(uri);
        self.send_player_command(&format!("play:{}", encoded))
    }

    fn play(&self) -> Result<(), ControlPointError> {
        self.send_player_command("resume")
    }

    fn pause(&self) -> Result<(), ControlPointError> {
        self.send_player_command("pause")
    }

    fn stop(&self) -> Result<(), ControlPointError> {
        self.send_player_command("stop")
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<(), ControlPointError> {
        let secs = parse_hhmmss_strict(hhmmss)?;
        self.send_player_command(&format!("seek:{}", secs))
    }
}

impl VolumeControl for LinkPlayRenderer {
    fn volume(&self) -> Result<u16, ControlPointError> {
        Ok(self.fetch_status()?.volume)
    }

    fn set_volume(&self, v: u16) -> Result<(), ControlPointError> {
        let value = v.min(100);
        self.send_player_command(&format!("vol:{}", value))
    }

    fn mute(&self) -> Result<bool, ControlPointError> {
        Ok(self.fetch_status()?.mute)
    }

    fn set_mute(&self, m: bool) -> Result<(), ControlPointError> {
        self.send_player_command(if m { "mute:1" } else { "mute:0" })
    }
}

impl PlaybackStatus for LinkPlayRenderer {
    fn playback_state(&self) -> Result<PlaybackState, ControlPointError> {
        Ok(self.fetch_status()?.playback_state())
    }
}

impl PlaybackPosition for LinkPlayRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo, ControlPointError> {
        Ok(self.fetch_status()?.position_info())
    }
}

impl RendererBackend for LinkPlayRenderer {
    fn queue(&self) -> &Arc<Mutex<MusicQueue>> {
        &self.queue
    }
}

impl QueueTransportControl for LinkPlayRenderer {
    fn play_from_queue(&self) -> Result<(), ControlPointError> {
        let mut queue = self.queue.lock().unwrap();

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

        let item = queue
            .get_item(current_index)?
            .ok_or_else(|| ControlPointError::QueueError("Current item not found".into()))?;

        let uri = item.uri.clone();
        drop(queue);

        self.play_uri(&uri, "")
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

impl QueueBackend for LinkPlayRenderer {
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
