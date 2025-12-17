use anyhow::{anyhow, Result};
use pmodidl::DIDLLite;
use quick_xml::escape::escape;
use tracing::debug;

use crate::media_server::ServerId;
use crate::model::RendererId;
use crate::openhome_client::{
    parse_track_metadata_from_didl, OhInfoClient, OhPlaylistClient, OhProductClient, OhTrackEntry,
    OPENHOME_PLAYLIST_HEAD_ID,
};
use crate::openhome_playlist::{OpenHomePlaylistSnapshot, OpenHomePlaylistTrack};
use crate::queue_backend::{PlaybackItem, QueueBackend, QueueSnapshot};

/// Local mirror of an OpenHome playlist for a single renderer.
#[derive(Clone, Debug)]
pub struct OpenHomeQueue {
    pub renderer_id: RendererId,
    pub playlist: OhPlaylistClient,
    pub info_client: Option<OhInfoClient>,
    pub product_client: Option<OhProductClient>,
    pub items: Vec<PlaybackItem>,
    pub current_index: Option<usize>,
    track_ids: Vec<u32>,
}

impl OpenHomeQueue {
    pub fn new(
        renderer_id: RendererId,
        playlist: OhPlaylistClient,
        info_client: Option<OhInfoClient>,
        product_client: Option<OhProductClient>,
    ) -> Self {
        Self {
            renderer_id,
            playlist,
            info_client,
            product_client,
            items: Vec::new(),
            current_index: None,
            track_ids: Vec::new(),
        }
    }

    /// Reload the full OpenHome playlist snapshot into local playback items.
    ///
    /// This mirrors the logic previously implemented by
    /// `OpenHomeRenderer::snapshot_openhome_playlist` but converts entries
    /// directly into `PlaybackItem`s.
    pub fn refresh_from_openhome(&mut self) -> Result<()> {
        self.ensure_playlist_source_selected()?;
        let entries = self.playlist.read_all_tracks()?;
        let mut items = Vec::with_capacity(entries.len());
        let mut track_ids = Vec::with_capacity(entries.len());

        for entry in &entries {
            items.push(self.playback_item_from_entry(entry));
            track_ids.push(entry.id);
        }

        let current_id = self
            .info_client
            .as_ref()
            .and_then(|client| client.id().ok());
        let previous_index = self
            .current_index
            .and_then(|idx| if idx < track_ids.len() { Some(idx) } else { None });
        let mut current_index = current_id
            .and_then(|id| track_ids.iter().position(|entry_id| *entry_id == id))
            .or(previous_index);

        if current_index.is_none() && !track_ids.is_empty() {
            current_index = Some(0);
        }

        self.items = items;
        self.track_ids = track_ids;
        self.current_index = current_index;
        Ok(())
    }

    pub fn openhome_playlist_snapshot(&self) -> Result<OpenHomePlaylistSnapshot> {
        let tracks = self
            .items
            .iter()
            .zip(self.track_ids.iter())
            .map(|(item, track_id)| OpenHomePlaylistTrack {
                id: *track_id,
                uri: item.uri.clone(),
                title: item.metadata.as_ref().and_then(|m| m.title.clone()),
                artist: item.metadata.as_ref().and_then(|m| m.artist.clone()),
                album: item.metadata.as_ref().and_then(|m| m.album.clone()),
                album_art_uri: item.metadata.as_ref().and_then(|m| m.album_art_uri.clone()),
            })
            .collect();

        Ok(OpenHomePlaylistSnapshot {
            renderer_id: self.renderer_id.0.clone(),
            current_id: self
                .current_index
                .and_then(|idx| self.track_ids.get(idx).copied()),
            current_index: self.current_index,
            tracks,
        })
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Return the list of OpenHome track IDs in order.
    pub fn openhome_track_ids(&self) -> Vec<u32> {
        self.track_ids.clone()
    }

    pub fn select_track_id(&mut self, id: u32) -> Result<()> {
        let index = match self.track_ids.iter().position(|&tid| tid == id) {
            Some(pos) => pos,
            None => {
                self.refresh_from_openhome()?;
                self.track_ids
                    .iter()
                    .position(|&tid| tid == id)
                    .ok_or_else(|| anyhow!("Unknown OpenHome track id {}", id))?
            }
        };

        self.ensure_playlist_source_selected()?;
        self.playlist.play_id(id)?;
        self.current_index = Some(index);
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        self.ensure_playlist_source_selected()?;
        self.playlist.delete_all()?;
        self.items.clear();
        self.track_ids.clear();
        self.current_index = None;
        Ok(())
    }

    /// Replace the remote OpenHome playlist entirely with `items`.
    ///
    /// This is used when attaching a media server playlist: we want to drop any
    /// stale entries (even if they were inserted by another control point) and
    /// rebuild the renderer playlist from scratch.
    pub fn replace_entire_playlist(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<()> {
        self.ensure_playlist_source_selected()?;
        self.playlist.delete_all()?;
        self.items.clear();
        self.track_ids.clear();
        self.current_index = None;

        if items.is_empty() {
            return Ok(());
        }

        let mut rebuilt_items = Vec::with_capacity(items.len());
        let mut rebuilt_ids = Vec::with_capacity(items.len());
        let mut previous_id = OPENHOME_PLAYLIST_HEAD_ID;

        for item in items {
            let metadata = build_metadata_xml(&item);
            let new_id = self.playlist.insert(previous_id, &item.uri, &metadata)?;
            previous_id = new_id;
            rebuilt_ids.push(new_id);
            rebuilt_items.push(self.item_with_openhome_id(item, new_id));
        }

        let normalized = current_index
            .filter(|&idx| idx < rebuilt_ids.len())
            .or_else(|| Some(0));

        self.items = rebuilt_items;
        self.track_ids = rebuilt_ids;
        self.current_index = normalized;
        Ok(())
    }

    pub fn add_playback_item(
        &mut self,
        item: PlaybackItem,
        after_id: Option<u32>,
        play: bool,
    ) -> Result<u32> {
        self.ensure_playlist_source_selected()?;
        let metadata_xml = build_metadata_xml(&item);
        let insert_after = match after_id {
            Some(id) => id,
            None => self.track_ids.last().copied().unwrap_or(0),
        };

        let new_id = self
            .playlist
            .insert(insert_after, &item.uri, &metadata_xml)?;

        if play {
            self.playlist.play_id(new_id)?;
        }

        let mut insert_index = after_id
            .and_then(|id| {
                if id == 0 {
                    Some(0)
                } else {
                    self.track_ids
                        .iter()
                        .position(|tid| *tid == id)
                        .map(|pos| pos + 1)
                }
            })
            .unwrap_or_else(|| self.track_ids.len());

        if insert_index > self.track_ids.len() {
            insert_index = self.track_ids.len();
        }

        self.track_ids.insert(insert_index, new_id);
        let stored_item = self.item_with_openhome_id(item, new_id);
        self.items.insert(insert_index, stored_item);

        self.current_index = if play {
            Some(insert_index)
        } else {
            self.current_index
                .map(|idx| if insert_index <= idx { idx + 1 } else { idx })
        };

        Ok(new_id)
    }

    fn ensure_playlist_source_selected(&self) -> Result<()> {
        if let Some(product) = &self.product_client {
            product.ensure_playlist_source_selected().map_err(|err| {
                anyhow!(
                    "Failed to select OpenHome Playlist source for {}: {}",
                    self.renderer_id.0,
                    err
                )
            })
        } else {
            Ok(())
        }
    }

    fn playback_item_from_entry(&self, entry: &OhTrackEntry) -> PlaybackItem {
        let metadata = parse_track_metadata_from_didl(&entry.metadata_xml);
        let didl_id = didl_id_from_metadata(&entry.metadata_xml)
            .unwrap_or_else(|| format!("openhome:{}", entry.id));
        PlaybackItem {
            media_server_id: ServerId(format!("openhome:{}", self.renderer_id.0)),
            didl_id,
            uri: entry.uri.clone(),
            // OpenHome tracks don't provide protocolInfo, use generic default
            protocol_info: "http-get:*:audio/*:*".to_string(),
            metadata,
        }
    }

    fn item_with_openhome_id(&self, mut item: PlaybackItem, track_id: u32) -> PlaybackItem {
        item.didl_id = format!("openhome:{}", track_id);
        item.media_server_id = ServerId(format!("openhome:{}", self.renderer_id.0));
        item
    }

    fn ensure_track_id(&mut self, index: usize) -> Result<u32> {
        if index >= self.items.len() {
            return Err(anyhow!("Index out of bounds in OpenHomeQueue: {}", index));
        }

        if let Some(id) = self.track_ids.get(index).copied() {
            return Ok(id);
        }

        self.refresh_from_openhome()?;
        self.track_ids
            .get(index)
            .copied()
            .ok_or_else(|| anyhow!("Failed to resolve OpenHome track id at index {}", index))
    }
}

pub fn didl_id_from_metadata(xml: &str) -> Option<String> {
    if xml.trim().is_empty() {
        return None;
    }

    let parsed = pmodidl::parse_metadata::<DIDLLite>(xml).ok()?;
    parsed.data.items.first().map(|item| item.id.clone())
}

fn build_metadata_xml(item: &PlaybackItem) -> String {
    let title = item
        .metadata
        .as_ref()
        .and_then(|m| m.title.as_deref())
        .unwrap_or("Unknown");
    let escaped_title = escape(title);
    let escaped_uri = escape(item.uri.as_str());
    let escaped_id = escape(item.didl_id.as_str());

    let mut xml = String::from(
        r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">"#,
    );
    xml.push_str(&format!(
        r#"<item id="{}" parentID="-1" restricted="1">"#,
        escaped_id
    ));
    xml.push_str(&format!("<dc:title>{}</dc:title>", escaped_title));

    if let Some(meta) = &item.metadata {
        if let Some(artist) = meta.artist.as_deref() {
            let escaped = escape(artist);
            xml.push_str(&format!("<upnp:artist>{}</upnp:artist>", escaped));
            xml.push_str(&format!("<dc:creator>{}</dc:creator>", escaped));
        }
        if let Some(album) = meta.album.as_deref() {
            let escaped = escape(album);
            xml.push_str(&format!("<upnp:album>{}</upnp:album>", escaped));
        }
        if let Some(genre) = meta.genre.as_deref() {
            let escaped = escape(genre);
            xml.push_str(&format!("<upnp:genre>{}</upnp:genre>", escaped));
        }
        if let Some(uri) = meta.album_art_uri.as_deref() {
            let escaped = escape(uri);
            xml.push_str(&format!("<upnp:albumArtURI>{}</upnp:albumArtURI>", escaped));
        }
        if let Some(date) = meta.date.as_deref() {
            let escaped = escape(date);
            xml.push_str(&format!("<dc:date>{}</dc:date>", escaped));
        }
        if let Some(track_no) = meta.track_number.as_deref() {
            let escaped = escape(track_no);
            xml.push_str(&format!(
                "<upnp:originalTrackNumber>{}</upnp:originalTrackNumber>",
                escaped
            ));
        }
    }

    let escaped_protocol_info = escape(item.protocol_info.as_str());
    xml.push_str(&format!(
        r#"<res protocolInfo="{}">{}</res>"#,
        escaped_protocol_info, escaped_uri
    ));
    xml.push_str(r#"<upnp:class>object.item.audioItem.musicTrack</upnp:class></item></DIDL-Lite>"#);
    xml
}

fn lcs_flags(current: &[PlaybackItem], desired: &[PlaybackItem]) -> (Vec<bool>, Vec<bool>) {
    let m = current.len();
    let n = desired.len();
    let mut dp = vec![vec![0u32; n + 1]; m + 1];

    for i in 0..m {
        for j in 0..n {
            if current[i].uri == desired[j].uri {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }

    let mut keep_current = vec![false; m];
    let mut keep_desired = vec![false; n];
    let (mut i, mut j) = (m, n);

    while i > 0 && j > 0 {
        if current[i - 1].uri == desired[j - 1].uri {
            keep_current[i - 1] = true;
            keep_desired[j - 1] = true;
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }

    (keep_current, keep_desired)
}

impl QueueBackend for OpenHomeQueue {
    fn queue_snapshot(&self) -> Result<QueueSnapshot> {
        Ok(QueueSnapshot {
            items: self.items.clone(),
            current_index: self.current_index,
        })
    }

    fn set_index(&mut self, index: Option<usize>) -> Result<()> {
        let normalized = index.filter(|&i| i < self.items.len());
        if let Some(idx) = normalized {
            let track_id = self.ensure_track_id(idx)?;
            self.ensure_playlist_source_selected()?;
            self.playlist.play_id(track_id)?;
        }
        self.current_index = normalized;
        Ok(())
    }

    fn replace_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<()> {
        self.ensure_playlist_source_selected()?;
        if items.is_empty() {
            self.playlist.delete_all()?;
            self.items.clear();
            self.track_ids.clear();
            self.current_index = None;
            return Ok(());
        }

        // Synchronize local state with the actual OpenHome playlist before computing
        // differences. Without this, any drift between our cache and the renderer
        // (e.g., manual edits from another control point) would keep the stale items.
        self.refresh_from_openhome()?;
        debug!(
            renderer = self.renderer_id.0.as_str(),
            actual_items = self.items.len(),
            "OpenHome playlist state refreshed before replace_queue"
        );

        let (keep_current, keep_desired) = lcs_flags(&self.items, &items);

        let items_to_keep = keep_current.iter().filter(|&&k| k).count();
        let items_to_delete = keep_current.iter().filter(|&&k| !k).count();
        let items_to_add = keep_desired.iter().filter(|&&k| !k).count();

        debug!(
            renderer = self.renderer_id.0.as_str(),
            keep = items_to_keep,
            delete = items_to_delete,
            add = items_to_add,
            "LCS computed: minimizing OpenHome playlist operations"
        );

        for idx in (0..self.track_ids.len()).rev() {
            if !keep_current[idx] {
                let track_id = self.track_ids[idx];
                self.playlist.delete_id(track_id)?;
                self.track_ids.remove(idx);
                self.items.remove(idx);
            }
        }

        let remaining_ids = self.track_ids.clone();
        let mut remaining_idx = 0usize;
        let mut previous_id = OPENHOME_PLAYLIST_HEAD_ID;
        let mut rebuilt_items = Vec::with_capacity(items.len());
        let mut rebuilt_ids = Vec::with_capacity(items.len());

        for (idx, item) in items.into_iter().enumerate() {
            if keep_desired[idx] {
                if remaining_idx >= remaining_ids.len() {
                    return Err(anyhow!(
                        "OpenHome playlist refresh bookkeeping mismatch (kept entries underflow)"
                    ));
                }
                let existing_id = remaining_ids[remaining_idx];
                remaining_idx += 1;
                previous_id = existing_id;
                rebuilt_ids.push(existing_id);
                rebuilt_items.push(self.item_with_openhome_id(item, existing_id));
            } else {
                let metadata = build_metadata_xml(&item);
                let new_id = self.playlist.insert(previous_id, &item.uri, &metadata)?;
                previous_id = new_id;
                rebuilt_ids.push(new_id);
                rebuilt_items.push(self.item_with_openhome_id(item, new_id));
            }
        }

        if remaining_idx != remaining_ids.len() {
            return Err(anyhow!(
                "OpenHome playlist refresh bookkeeping mismatch (kept entries overflow)"
            ));
        }

        let previous_index = self
            .current_index
            .and_then(|idx| if idx < rebuilt_ids.len() { Some(idx) } else { None });
        let normalized = current_index
            .filter(|&i| i < rebuilt_ids.len())
            .or(previous_index)
            .or_else(|| if rebuilt_ids.is_empty() { None } else { Some(0) });
        self.items = rebuilt_items;
        self.track_ids = rebuilt_ids;
        self.current_index = normalized;
        Ok(())
    }

    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>> {
        Ok(self.items.get(index).cloned())
    }

    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<()> {
        if index >= self.items.len() {
            return Ok(());
        }

        self.ensure_playlist_source_selected()?;
        let track_id = self.ensure_track_id(index)?;
        let before_id = if index == 0 {
            OPENHOME_PLAYLIST_HEAD_ID
        } else {
            self.ensure_track_id(index - 1)?
        };

        self.playlist.delete_id(track_id)?;
        let metadata = build_metadata_xml(&item);
        let new_id = self.playlist.insert(before_id, &item.uri, &metadata)?;

        if self.current_index == Some(index) {
            self.playlist.play_id(new_id)?;
        }

        self.items[index] = self.item_with_openhome_id(item, new_id);
        self.track_ids[index] = new_id;
        Ok(())
    }
}
