use std::usize;

use quick_xml::escape::escape;
use tracing::debug;

use crate::errors::ControlPointError;
use crate::upnp_clients::{
    OPENHOME_PLAYLIST_HEAD_ID, OhInfoClient, OhPlaylistClient, OhProductClient, OhTrack,
    OhTrackEntry,
};
// use crate::openhome_playlist::{OpenHomePlaylistSnapshot, OpenHomePlaylistTrack};
use crate::queue::{
    EnqueueMode, MusicQueue, PlaybackItem, QueueBackend, QueueFromRendererInfo, QueueSnapshot,
};
use crate::{DeviceId, DeviceIdentity, RendererInfo};

/// Local mirror of an OpenHome playlist for a single renderer.
#[derive(Clone, Debug)]
pub struct OpenHomeQueue {
    renderer_id: DeviceId,
    playlist_client: OhPlaylistClient,
    info_client: Option<OhInfoClient>,
    product_client: Option<OhProductClient>,
    // current_index: Option<usize>,
}

impl OpenHomeQueue {
    pub fn new(
        renderer_id: DeviceId,
        playlist: OhPlaylistClient,
        info_client: Option<OhInfoClient>,
        product_client: Option<OhProductClient>,
    ) -> Self {
        Self {
            renderer_id,
            playlist_client: playlist,
            info_client,
            product_client,
        }
    }

    pub fn from_renderer_info(info: &RendererInfo) -> Result<OpenHomeQueue, ControlPointError> {
        let playlist_client = OhPlaylistClient::from_renderer_info(info)?;
        let info_client = OhInfoClient::from_renderer_info(&info).ok();
        let product_client = OhProductClient::from_renderer_info(&info).ok();

        Ok(OpenHomeQueue::new(
            info.id(),
            playlist_client,
            info_client,
            product_client,
        ))
    }

    fn ensure_playlist_source_selected(&self) -> Result<(), ControlPointError> {
        if let Some(product) = &self.product_client {
            product.ensure_playlist_source_selected()
        } else {
            Ok(())
        }
    }

    fn playback_item_from_entry(&self, entry: &OhTrackEntry) -> PlaybackItem {
        let metadata = entry.metadata();
        let didl_id = entry
            .didl_id()
            .unwrap_or_else(|| format!("openhome:{}", entry.id));
        PlaybackItem {
            media_server_id: DeviceId(format!("openhome:{}", self.renderer_id.0)),
            backend_id: entry.id as usize,
            didl_id,
            uri: entry.uri().to_string(),
            // OpenHome tracks don't provide protocolInfo, use generic default
            protocol_info: "http-get:*:audio/*:*".to_string(),
            metadata,
        }
    }

    fn item_with_openhome_id(&self, mut item: PlaybackItem, track_id: u32) -> PlaybackItem {
        item.didl_id = format!("openhome:{}", track_id);
        item.media_server_id = DeviceId(format!("openhome:{}", self.renderer_id.0));
        item
    }

    fn add_playback_item(
        &mut self,
        item: PlaybackItem,
        after_id: u32,
    ) -> Result<u32, ControlPointError> {
        self.ensure_playlist_source_selected()?;
        let metadata_xml = build_metadata_xml(&item);
        let new_id = self
            .playlist_client
            .insert(after_id, &item.uri, &metadata_xml)?;
        Ok(new_id)
    }

    /// CASE 1: Replace queue while preserving the currently playing item as first.
    /// The currently playing item is NOT in the new playlist, so we keep it as the first
    /// item and append the entire new playlist after it.
    fn replace_queue_preserve_current(
        &mut self,
        new_items: Vec<PlaybackItem>,
        playing_id: usize,
    ) -> Result<(), ControlPointError> {
        // Get current track IDs from OpenHome
        let current_track_ids = self.track_ids()?;

        // Delete everything except the currently playing item
        // Using delete_id_if_exists() to handle cases where another control point
        // may have already modified the playlist
        for &track_id in current_track_ids.iter().rev() {
            if track_id as usize != playing_id {
                self.playlist_client.delete_id_if_exists(track_id)?;
            }
        }

        // Insert new items after the currently playing track
        let mut previous_id = playing_id as u32;
        for item in new_items {
            let metadata = build_metadata_xml(&item);
            let new_id = self
                .playlist_client
                .insert(previous_id, &item.uri, &metadata)?;
            previous_id = new_id;
        }

        debug!(
            renderer = self.renderer_id.0.as_str(),
            "Gentle sync completed: preserved playing track as first item (not in new playlist)"
        );

        Ok(())
    }

    /// Helper: Delete items marked for deletion in reverse order with logging.
    fn delete_marked_items(
        &mut self,
        old_ids: &[u32],
        keep_flags: &[bool],
        position_label: &str,
    ) -> Result<(), ControlPointError> {
        for (idx, &track_id) in old_ids.iter().enumerate().rev() {
            if !keep_flags[idx] {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    track_id,
                    position = position_label,
                    "RENDERER OP: DeleteId({})",
                    track_id
                );
                self.playlist_client.delete_id_if_exists(track_id)?;
            }
        }
        Ok(())
    }

    /// Helper: Rebuild a playlist section (before or after pivot) using LCS results.
    fn rebuild_playlist_section(
        &mut self,
        new_items: &[PlaybackItem],
        keep_new_flags: &[bool],
        old_ids: &[u32],
        keep_old_flags: &[bool],
        mut previous_id: u32,
        position_label: &str,
    ) -> Result<u32, ControlPointError> {
        // Collect IDs of kept items (in order)
        let remaining_ids: Vec<u32> = old_ids
            .iter()
            .enumerate()
            .filter_map(|(idx, &id)| if keep_old_flags[idx] { Some(id) } else { None })
            .collect();

        let mut remaining_idx = 0;

        // Rebuild section
        for (idx, item) in new_items.iter().enumerate() {
            if keep_new_flags[idx] {
                let existing_id = remaining_ids[remaining_idx];
                remaining_idx += 1;
                previous_id = existing_id;
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    track_id = existing_id,
                    position = position_label,
                    "KEPT existing track ID {}",
                    existing_id
                );
            } else {
                let metadata = build_metadata_xml(item);
                let new_id = self
                    .playlist_client
                    .insert(previous_id, &item.uri, &metadata)?;
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    after_id = previous_id,
                    new_id,
                    position = position_label,
                    "RENDERER OP: Insert(after={}) -> new_id={}",
                    previous_id,
                    new_id
                );
                previous_id = new_id;
            }
        }

        Ok(previous_id)
    }

    /// CASE 2: Replace queue with double-LCS (before and after the pivot).
    /// The currently playing item IS in the new playlist, so we use it as a pivot
    /// and apply LCS separately to the portions before and after it.
    fn replace_queue_with_pivot(
        &mut self,
        new_items: Vec<PlaybackItem>,
        pivot_idx_new: usize,
        pivot_id: usize,
    ) -> Result<(), ControlPointError> {
        // Get current state from OpenHome
        let snapshot = self.queue_snapshot()?;
        let current_track_ids = self.track_ids()?;

        // Find the pivot index in our current state
        let pivot_idx = current_track_ids
            .iter()
            .position(|&id| id as usize == pivot_id)
            .ok_or_else(|| {
                ControlPointError::OpenHomeError(format!(
                    "Pivot track ID {} not found in playlist",
                    pivot_id
                ))
            })?;

        // Split current data at the pivot
        let old_before: Vec<PlaybackItem> = snapshot.items[..pivot_idx].to_vec();
        let old_after: Vec<PlaybackItem> = snapshot.items[pivot_idx + 1..].to_vec();
        let old_ids_before: Vec<u32> = current_track_ids[..pivot_idx].to_vec();
        let old_ids_after: Vec<u32> = current_track_ids[pivot_idx + 1..].to_vec();

        let new_before = &new_items[..pivot_idx_new];
        let new_after = &new_items[pivot_idx_new + 1..];

        // LCS on the AFTER part (using fresh data from OpenHome)
        let (keep_old_after, keep_new_after) = lcs_flags(&old_after, new_after);

        // LCS on the BEFORE part (using fresh data from OpenHome)
        let (keep_old_before, keep_new_before) = lcs_flags(&old_before, new_before);

        // Delete items marked for deletion in AFTER part (reverse order)
        self.delete_marked_items(&old_ids_after, &keep_old_after, "AFTER pivot")?;

        // Delete items marked for deletion in BEFORE part (reverse order)
        self.delete_marked_items(&old_ids_before, &keep_old_before, "BEFORE pivot")?;

        // Rebuild the playlist: [BEFORE, PIVOT, AFTER]
        // Rebuild BEFORE part (we don't need the returned previous_id)
        self.rebuild_playlist_section(
            new_before,
            &keep_new_before,
            &old_ids_before,
            &keep_old_before,
            OPENHOME_PLAYLIST_HEAD_ID,
            "BEFORE pivot",
        )?;

        // PIVOT keeps its ID and position - it's the anchor point
        let previous_id = pivot_id as u32;
        debug!(
            renderer = self.renderer_id.0.as_str(),
            pivot_id,
            pivot_idx_new,
            "PIVOT preserved with ID {} at index {}",
            pivot_id,
            pivot_idx_new
        );

        // Rebuild AFTER part
        self.rebuild_playlist_section(
            new_after,
            &keep_new_after,
            &old_ids_after,
            &keep_old_after,
            previous_id,
            "AFTER pivot",
        )?;

        debug!(
            renderer = self.renderer_id.0.as_str(),
            pivot_idx = pivot_idx_new,
            pivot_id,
            "Gentle sync completed: double-LCS with pivot (playing track preserved)"
        );

        Ok(())
    }

    /// Standard LCS-based replacement (used when no currently playing item).
    fn replace_queue_standard_lcs(
        &mut self,
        items: Vec<PlaybackItem>,
        _current_index: Option<usize>,
    ) -> Result<(), ControlPointError> {
        // Get current state from OpenHome
        let snapshot = self.queue_snapshot()?;
        let current_track_ids = self.track_ids()?;

        let (keep_current, keep_desired) = lcs_flags(&snapshot.items, &items);

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

        // If we're replacing everything (keep=0), use delete_all() instead of
        // individual delete_id() calls. This is much more robust for live playlists
        // where track IDs can become invalid between refresh and deletion.
        if items_to_keep == 0 && items_to_delete > 0 {
            debug!(
                renderer = self.renderer_id.0.as_str(),
                "Using delete_all() for complete replacement (more robust for live playlists)"
            );
            self.playlist_client.delete_all()?;
        } else {
            // Selective deletion when keeping some items
            for idx in (0..current_track_ids.len()).rev() {
                if !keep_current[idx] {
                    let track_id = current_track_ids[idx];
                    // Use delete_id_if_exists() to handle cases where another control point
                    // may have already modified the playlist
                    self.playlist_client.delete_id_if_exists(track_id)?;
                }
            }
        }

        // Rebuild by inserting new items
        let remaining_ids: Vec<u32> = current_track_ids
            .iter()
            .enumerate()
            .filter_map(|(idx, &id)| {
                if keep_current.get(idx).copied().unwrap_or(false) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect();

        let mut remaining_idx = 0usize;
        let mut previous_id = OPENHOME_PLAYLIST_HEAD_ID;

        for (idx, item) in items.into_iter().enumerate() {
            if keep_desired[idx] {
                if remaining_idx >= remaining_ids.len() {
                    return Err(ControlPointError::OpenHomeError(format!(
                        "OpenHome playlist refresh bookkeeping mismatch (kept entries underflow)"
                    )));
                }
                let existing_id = remaining_ids[remaining_idx];
                remaining_idx += 1;
                previous_id = existing_id;
            } else {
                let metadata = build_metadata_xml(&item);
                let new_id = self
                    .playlist_client
                    .insert(previous_id, &item.uri, &metadata)?;
                previous_id = new_id;
            }
        }

        if remaining_idx != remaining_ids.len() {
            return Err(ControlPointError::OpenHomeError(format!(
                "OpenHome playlist refresh bookkeeping mismatch (kept entries overflow)"
            )));
        }

        Ok(())
    }
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

/// Compare two PlaybackItems for equality.
/// Items are considered equal if they have the same URI OR the same didl_id.
/// This allows matching items even when the MediaServer returns different URIs
/// for the same logical track (e.g., with session tokens or different encodings).
fn items_match(a: &PlaybackItem, b: &PlaybackItem) -> bool {
    a.uri == b.uri || a.didl_id == b.didl_id
}

fn lcs_flags(current: &[PlaybackItem], desired: &[PlaybackItem]) -> (Vec<bool>, Vec<bool>) {
    let m = current.len();
    let n = desired.len();
    let mut dp = vec![vec![0u32; n + 1]; m + 1];

    for i in 0..m {
        for j in 0..n {
            if items_match(&current[i], &desired[j]) {
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
        if items_match(&current[i - 1], &desired[j - 1]) {
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
    fn len(&self) -> Result<usize, ControlPointError> {
        Ok(self.track_ids()?.len())
    }

    /// Return the list of OpenHome track IDs in order.
    fn track_ids(&self) -> Result<Vec<u32>, ControlPointError> {
        self.ensure_playlist_source_selected()?;
        self.playlist_client.id_array()
    }

    fn id_to_position(&self, id: u32) -> Result<usize, ControlPointError> {
        self.track_ids()?
            .iter()
            .position(|&tid| tid == id)
            .ok_or_else(|| {
                ControlPointError::QueueError(format!("Item {} id is not present in the queue", id))
            })
    }

    fn position_to_id(&self, index: usize) -> Result<u32, ControlPointError> {
        let idxs = self.track_ids()?;

        if index < idxs.len() {
            Ok(idxs[index])
        } else {
            Err(ControlPointError::QueueError(format!(
                "Index out of bound {} >= {}",
                index,
                idxs.len()
            )))
        }
    }

    fn current_track(&self) -> Result<Option<u32>, ControlPointError> {
        let id = self.playlist_client.id()?;
        // OpenHome returns 0 when no track is selected/playing
        if id == 0 { Ok(None) } else { Ok(Some(id)) }
    }

    fn current_index(&self) -> Result<Option<usize>, ControlPointError> {
        if let Some(id) = self.current_track()? {
            return Ok(Some(self.id_to_position(id)?));
        }

        Ok(None)
    }

    fn queue_snapshot(&self) -> Result<QueueSnapshot, ControlPointError> {
        self.ensure_playlist_source_selected()?;
        let entries = self.playlist_client.read_all_tracks()?;
        let mut items = Vec::with_capacity(entries.len());

        for entry in &entries {
            items.push(self.playback_item_from_entry(entry));
        }

        // Get the currently playing track ID from the renderer (may be None if no track is playing)
        let current_id = self.playlist_client.id().ok();

        // Find the index of the current track in the playlist
        let current_index = current_id.and_then(|id| {
            items
                .iter()
                .position(|entry_id| entry_id.backend_id == id as usize)
        });

        Ok(QueueSnapshot {
            items: items,
            current_index: current_index,
            playlist_id: None,
        })
    }

    fn set_index(&mut self, index: Option<usize>) -> Result<(), ControlPointError> {
        if let Some(index) = index {
            let track_id = self.position_to_id(index)?;
            self.ensure_playlist_source_selected()?;
            self.playlist_client.seek_id(track_id)?;
        } else {
            self.ensure_playlist_source_selected()?;
            self.playlist_client.stop()?;
        }
        Ok(())
    }

    fn replace_queue(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<(), ControlPointError> {
        if let Some(ci) = current_index {
            if ci >= items.len() {
                return Err(ControlPointError::QueueError(format!(
                    "Invalid Current index parameter {} >= {}",
                    ci,
                    items.len()
                )));
            }
        }

        self.ensure_playlist_source_selected()?;
        self.playlist_client.delete_all()?;

        if items.is_empty() {
            return Ok(());
        }

        let mut previous_id = OPENHOME_PLAYLIST_HEAD_ID;

        for item in items {
            let metadata = build_metadata_xml(&item);
            let new_id = self
                .playlist_client
                .insert(previous_id, &item.uri, &metadata)?;
            previous_id = new_id;
        }

        Ok(())
    }

    fn sync_queue(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        self.ensure_playlist_source_selected()?;
        if items.is_empty() {
            self.playlist_client.delete_all()?;
            return Ok(());
        }

        // Synchronize local state with the actual OpenHome playlist before computing
        // differences. Without this, any drift between our cache and the renderer
        // (e.g., manual edits from another control point) would keep the stale items.
        let snapshot = self.queue_snapshot()?;
        let playing_info = snapshot.current_index.and_then(|idx| {
            Some((
                idx,
                snapshot.items[idx].backend_id,
                snapshot.items[idx].uri.clone(),
                snapshot.items[idx].didl_id.clone(),
            ))
        });

        debug!(
            renderer = self.renderer_id.0.as_str(),
            actual_items = snapshot.items.len(),
            playing_info_detected = playing_info.is_some(),
            "OpenHome playlist state"
        );

        if let Some((playing_idx, playing_id, playing_uri, playing_didl_id)) = playing_info {
            // Find if the currently playing item is in the new playlist (by URI first, then by didl_id)
            let new_playing_idx = items
                .iter()
                .position(|item| item.uri == playing_uri)
                .or_else(|| {
                    items
                        .iter()
                        .position(|item| item.didl_id == playing_didl_id)
                });

            if let Some(pivot_idx) = new_playing_idx {
                // CASE 2: Currently playing item IS in the new playlist
                // Use gentle double-LCS strategy: preserve the pivot and sync before/after separately
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    playing_idx,
                    pivot_idx,
                    "Gentle sync: currently playing item found in new playlist at index {}",
                    pivot_idx
                );

                self.replace_queue_with_pivot(items, pivot_idx, playing_id)?;
            } else {
                // CASE 1: Currently playing item NOT in the new playlist
                // Keep it as first item and append the new playlist after it
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    playing_idx,
                    "Gentle sync: currently playing item not in new playlist, preserving as first item"
                );

                self.replace_queue_preserve_current(items, playing_id)?;
            }
        } else {
            // No currently playing item or can't determine it - use standard LCS
            debug!(
                renderer = self.renderer_id.0.as_str(),
                "No currently playing item, using standard LCS sync"
            );
            self.replace_queue_standard_lcs(items, Some(0))?;
        }

        Ok(())
    }

    fn get_item(&self, index: usize) -> Result<Option<PlaybackItem>, ControlPointError> {
        let snapshot = self.queue_snapshot()?;

        if index < snapshot.items.len() {
            return Ok(Some(snapshot.items[index].clone()));
        }

        Err(ControlPointError::QueueError(format!(
            "get_item index out of bound {} >= {}",
            index,
            snapshot.items.len()
        )))
    }

    fn replace_item(&mut self, index: usize, item: PlaybackItem) -> Result<(), ControlPointError> {
        let metadata = build_metadata_xml(&item);

        let ids = self.track_ids()?;

        if index >= ids.len() {
            return Err(ControlPointError::QueueError(format!(
                "get_item index out of bound {} >= {}",
                index,
                ids.len()
            )));
        }

        self.ensure_playlist_source_selected()?;

        let track_id = ids[index];
        let before_id = if index == 0 {
            OPENHOME_PLAYLIST_HEAD_ID
        } else {
            ids[index - 1]
        };

        let ci = self.current_index()?;
        // Use delete_id_if_exists() to handle cases where another control point
        // may have already modified the playlist
        self.playlist_client.delete_id(track_id)?;
        let new_id = self
            .playlist_client
            .insert(before_id, &item.uri, &metadata)?;

        if ci == Some(index) {
            self.playlist_client.seek_id(new_id)?;
        }

        Ok(())
    }

    /// Override enqueue_items to add items directly to the OpenHome playlist.
    fn enqueue_items(
        &mut self,
        items: Vec<PlaybackItem>,
        mode: EnqueueMode,
    ) -> Result<(), ControlPointError> {
        if items.is_empty() {
            return Ok(());
        }

        match mode {
            EnqueueMode::AppendToEnd => {
                let ids = self.track_ids()?;
                let mut after_id = if ids.len() > 0 {
                    ids[ids.len() - 1]
                } else {
                    OPENHOME_PLAYLIST_HEAD_ID
                };
                // Append to the end of the OpenHome playlist
                for item in items {
                    after_id = self.add_playback_item(item, after_id)?;
                }
            }
            EnqueueMode::InsertAfterCurrent => {
                if let Some(mut after_id) = self.current_track()? {
                    for item in items {
                        after_id = self.add_playback_item(item, after_id)?;
                    }
                } else {
                    self.enqueue_items(items, EnqueueMode::AppendToEnd)?;
                }
            }
            EnqueueMode::ReplaceAll => {
                // Replace the entire playlist
                self.replace_queue(items, None)?;
            }
        }
        Ok(())
    }

    // Optimized helpers to avoid unnecessary network calls

    /// Optimized clear_queue: use delete_all() directly instead of replace_queue.
    fn clear_queue(&mut self) -> Result<(), ControlPointError> {
        self.ensure_playlist_source_selected()?;
        self.playlist_client.delete_all()
    }

    /// Optimized is_empty: only fetch track IDs, not the full playlist.
    fn is_empty(&self) -> Result<bool, ControlPointError> {
        Ok(self.track_ids()?.is_empty())
    }

    /// Optimized upcoming_len: calculate from len() and current_index() without fetching items.
    fn upcoming_len(&self) -> Result<usize, ControlPointError> {
        let len = self.len()?;
        match self.current_index()? {
            None => Ok(len),
            Some(idx) => Ok(len.saturating_sub(idx + 1)),
        }
    }

    /// Optimized peek_current: use primitives instead of full snapshot.
    fn peek_current(&mut self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        let len = self.len()?;
        if len == 0 {
            return Ok(None);
        }

        let current_idx = self.current_index()?;
        let resolved_index = match current_idx {
            Some(idx) if idx < len => Some(idx),
            _ => None,
        };

        let item_index = resolved_index.unwrap_or(0);
        let item = match self.get_item(item_index)? {
            Some(item) => item,
            None => return Ok(None),
        };

        let remaining = match resolved_index {
            Some(idx) => len.saturating_sub(idx + 1),
            None => len,
        };

        Ok(Some((item, remaining)))
    }

    /// Optimized dequeue_next: use primitives instead of full snapshot.
    fn dequeue_next(&mut self) -> Result<Option<(PlaybackItem, usize)>, ControlPointError> {
        let len = self.len()?;
        if len == 0 {
            return Ok(None);
        }

        let current_idx = self.current_index()?;
        let next_index = match current_idx {
            None => 0,
            Some(idx) => {
                let candidate = idx + 1;
                if candidate >= len {
                    return Ok(None);
                }
                candidate
            }
        };

        let Some(item) = self.get_item(next_index)? else {
            return Ok(None);
        };

        let remaining = len.saturating_sub(next_index + 1);
        self.set_index(Some(next_index))?;
        Ok(Some((item, remaining)))
    }

    /// Optimized append_or_init_index: use enqueue_items which is already optimized.
    fn append_or_init_index(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError> {
        let was_empty = self.is_empty()?;

        // Use the already optimized enqueue_items(AppendToEnd)
        self.enqueue_items(items, EnqueueMode::AppendToEnd)?;

        // If the queue was empty before, set index to 0
        if was_empty && !self.is_empty()? {
            self.set_index(Some(0))?;
        }

        Ok(())
    }
}

impl QueueFromRendererInfo for OpenHomeQueue {
    fn from_renderer_info(renderer: &RendererInfo) -> Result<Self, ControlPointError> {
        OpenHomeQueue::from_renderer_info(renderer)
    }

    fn to_backend(self) -> MusicQueue {
        MusicQueue::OpenHome(self)
    }
}
