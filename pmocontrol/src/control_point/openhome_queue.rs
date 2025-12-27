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

        // Try multiple methods to determine the currently playing track, from most to least reliable:
        // 1. Info.Id() - Direct ID query (fastest, but fails if track no longer in playlist)
        // 2. Info.Track() - Returns URI, which we can search for (works even if track removed)
        // 3. None - No current track can be determined
        let current_id = self.playlist.id()?;
        //  {
        //     // Try Info.Id() first
        //     if let Ok(id) = client.id() {
        //         debug!(
        //             renderer = self.renderer_id.0.as_str(),
        //             track_id = id,
        //             "Detected current track via Info.Id()"
        //         );
        //         return Some(id);
        //     }

        //     // If Id() fails, try Track() to get the URI and search for it
        //     if let Ok(track_info) = client.track() {
        //         debug!(
        //             renderer = self.renderer_id.0.as_str(),
        //             track_uri = track_info.uri.as_str(),
        //             "Info.Id() failed, searching for current track by URI from Info.Track()"
        //         );
        //         return entries
        //             .iter()
        //             .find(|entry| entry.uri == track_info.uri)
        //             .map(|entry| {
        //                 debug!(
        //                     renderer = self.renderer_id.0.as_str(),
        //                     found_id = entry.id,
        //                     found_uri = entry.uri.as_str(),
        //                     "Found current track ID by matching URI"
        //                 );
        //                 entry.id
        //             });
        //     }

        //     debug!(
        //         renderer = self.renderer_id.0.as_str(),
        //         "Both Info.Id() and Info.Track() failed, cannot determine current track"
        //     );
        //     None
        // });

        // let current_index = current_id
        //     .and_then(|id| track_ids.iter().position(|entry_id| *entry_id == id));

        self.items = items;
        self.track_ids = track_ids;
        self.current_index = Some(current_id as usize);
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

    /// Selects and plays a track by its queue index (0-based).
    pub fn select_track_index(&mut self, index: usize) -> Result<()> {
        let track_id = self
            .track_ids
            .get(index)
            .copied()
            .ok_or_else(|| anyhow!("Index {} out of bounds (queue length: {})", index, self.track_ids.len()))?;

        self.ensure_playlist_source_selected()?;
        self.playlist.play_id(track_id)?;
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

    /// CASE 1: Replace queue while preserving the currently playing item as first.
    /// The currently playing item is NOT in the new playlist, so we keep it as the first
    /// item and append the entire new playlist after it.
    fn replace_queue_preserve_current(
        &mut self,
        new_items: Vec<PlaybackItem>,
        playing_idx: usize,
        playing_id: u32,
    ) -> Result<()> {
        // Re-read the current playlist state to get fresh IDs
        // This minimizes race conditions where IDs become invalid between our last refresh
        // and now (due to UPnP events from the server)
        let current_entries = self.playlist.read_all_tracks()?;
        let current_ids: Vec<u32> = current_entries.iter().map(|e| e.id).collect();

        debug!(
            renderer = self.renderer_id.0.as_str(),
            fresh_id_count = current_ids.len(),
            cached_id_count = self.track_ids.len(),
            "Re-read playlist before deletions to avoid stale ID errors"
        );

        // Find the playing track in the fresh list
        let fresh_playing_idx = current_ids.iter().position(|&id| id == playing_id);

        if fresh_playing_idx.is_none() {
            debug!(
                renderer = self.renderer_id.0.as_str(),
                playing_id,
                "Playing track not found in fresh playlist - renderer state may have changed, aborting modification"
            );
            // The playing track is gone - don't try to manipulate the playlist
            return Ok(());
        }

        // Delete everything except the currently playing item (using fresh IDs)
        for &track_id in current_ids.iter().rev() {
            if track_id != playing_id {
                self.playlist.delete_id_if_exists(track_id)?;
            }
        }

        // Rebuild: [currently_playing, new_items...]
        let mut rebuilt_items = Vec::with_capacity(1 + new_items.len());
        let mut rebuilt_ids = Vec::with_capacity(1 + new_items.len());

        rebuilt_items.push(self.items[playing_idx].clone());
        rebuilt_ids.push(playing_id);

        let mut previous_id = playing_id;
        for item in new_items {
            let metadata = build_metadata_xml(&item);
            let new_id = self.playlist.insert(previous_id, &item.uri, &metadata)?;
            previous_id = new_id;
            rebuilt_ids.push(new_id);
            rebuilt_items.push(self.item_with_openhome_id(item, new_id));
        }

        self.items = rebuilt_items;
        self.track_ids = rebuilt_ids;
        self.current_index = Some(0); // Currently playing is now at index 0

        debug!(
            renderer = self.renderer_id.0.as_str(),
            "Gentle sync completed: preserved playing track as first item (not in new playlist)"
        );

        Ok(())
    }

    /// CASE 2: Replace queue with double-LCS (before and after the pivot).
    /// The currently playing item IS in the new playlist, so we use it as a pivot
    /// and apply LCS separately to the portions before and after it.
    fn replace_queue_with_pivot(
        &mut self,
        new_items: Vec<PlaybackItem>,
        pivot_idx_new: usize,
        pivot_id: u32,
    ) -> Result<()> {
        debug!(
            renderer = self.renderer_id.0.as_str(),
            pivot_id,
            pivot_idx_new,
            new_playlist_len = new_items.len(),
            "Starting replace_queue_with_pivot - will re-read from OpenHome"
        );

        // Re-read the current playlist state from OpenHome (the ONLY source of truth)
        // This is CRITICAL to avoid deleting IDs that no longer exist, which can
        // put the renderer (upmpdcli) into a degraded state where Info.TransportState()
        // starts returning HTTP 500 errors.
        let current_entries = self.playlist.read_all_tracks()?;

        // Convert entries to PlaybackItems - this is the REAL current state
        let mut fresh_items = Vec::with_capacity(current_entries.len());
        let mut fresh_ids = Vec::with_capacity(current_entries.len());
        for entry in &current_entries {
            fresh_items.push(self.playback_item_from_entry(entry));
            fresh_ids.push(entry.id);
        }

        debug!(
            renderer = self.renderer_id.0.as_str(),
            fresh_count = fresh_items.len(),
            "Re-read playlist from OpenHome (source of truth)"
        );

        // Find the pivot in the fresh list
        let fresh_pivot_idx = fresh_ids.iter().position(|&id| id == pivot_id);

        if fresh_pivot_idx.is_none() {
            debug!(
                renderer = self.renderer_id.0.as_str(),
                pivot_id,
                "Pivot track not found in fresh playlist - renderer state changed, aborting"
            );
            return Ok(());
        }

        let fresh_pivot_idx = fresh_pivot_idx.unwrap();

        // Split fresh data at the pivot - use ONLY fresh data, ignore cache
        let old_before: Vec<PlaybackItem> = fresh_items[..fresh_pivot_idx].to_vec();
        let old_after: Vec<PlaybackItem> = fresh_items[fresh_pivot_idx + 1..].to_vec();
        let old_ids_before: Vec<u32> = fresh_ids[..fresh_pivot_idx].to_vec();
        let old_ids_after: Vec<u32> = fresh_ids[fresh_pivot_idx + 1..].to_vec();

        let new_before = &new_items[..pivot_idx_new];
        let new_after = &new_items[pivot_idx_new + 1..];

        // LCS on the AFTER part (using fresh data from OpenHome)
        let (keep_old_after, keep_new_after) = lcs_flags(&old_after, new_after);

        // LCS on the BEFORE part (using fresh data from OpenHome)
        let (keep_old_before, keep_new_before) = lcs_flags(&old_before, new_before);

        // Delete items marked for deletion in AFTER part (reverse order)
        for (idx, &track_id) in old_ids_after.iter().enumerate().rev() {
            if !keep_old_after[idx] {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    track_id,
                    position = "AFTER pivot",
                    "RENDERER OP: DeleteId({})",
                    track_id
                );
                self.playlist.delete_id_if_exists(track_id)?;
            }
        }

        // Delete items marked for deletion in BEFORE part (reverse order)
        for (idx, &track_id) in old_ids_before.iter().enumerate().rev() {
            if !keep_old_before[idx] {
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    track_id,
                    position = "BEFORE pivot",
                    "RENDERER OP: DeleteId({})",
                    track_id
                );
                self.playlist.delete_id_if_exists(track_id)?;
            }
        }

        // Rebuild the playlist: [BEFORE, PIVOT, AFTER]
        let mut rebuilt_items = Vec::with_capacity(new_items.len());
        let mut rebuilt_ids = Vec::with_capacity(new_items.len());

        // Collect IDs of kept items in BEFORE part (in order)
        let remaining_before: Vec<u32> = old_ids_before
            .iter()
            .enumerate()
            .filter_map(|(idx, &id)| if keep_old_before[idx] { Some(id) } else { None })
            .collect();

        let mut remaining_before_idx = 0;
        let mut previous_id = OPENHOME_PLAYLIST_HEAD_ID;

        // Rebuild BEFORE part
        for (idx, item) in new_before.iter().enumerate() {
            if keep_new_before[idx] {
                let existing_id = remaining_before[remaining_before_idx];
                remaining_before_idx += 1;
                previous_id = existing_id;
                rebuilt_ids.push(existing_id);
                rebuilt_items.push(self.item_with_openhome_id(item.clone(), existing_id));
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    track_id = existing_id,
                    position = "BEFORE pivot",
                    "KEPT existing track ID {}",
                    existing_id
                );
            } else {
                let metadata = build_metadata_xml(item);
                let new_id = self.playlist.insert(previous_id, &item.uri, &metadata)?;
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    after_id = previous_id,
                    new_id,
                    position = "BEFORE pivot",
                    "RENDERER OP: Insert(after={}) -> new_id={}",
                    previous_id,
                    new_id
                );
                previous_id = new_id;
                rebuilt_ids.push(new_id);
                rebuilt_items.push(self.item_with_openhome_id(item.clone(), new_id));
            }
        }

        // Add PIVOT (keeps its ID!)
        rebuilt_ids.push(pivot_id);
        rebuilt_items.push(self.item_with_openhome_id(new_items[pivot_idx_new].clone(), pivot_id));
        previous_id = pivot_id;
        debug!(
            renderer = self.renderer_id.0.as_str(),
            pivot_id,
            pivot_idx_new,
            "PIVOT preserved with ID {} at index {}",
            pivot_id,
            pivot_idx_new
        );

        // Collect IDs of kept items in AFTER part (in order)
        let remaining_after: Vec<u32> = old_ids_after
            .iter()
            .enumerate()
            .filter_map(|(idx, &id)| if keep_old_after[idx] { Some(id) } else { None })
            .collect();

        let mut remaining_after_idx = 0;

        // Rebuild AFTER part
        for (idx, item) in new_after.iter().enumerate() {
            if keep_new_after[idx] {
                let existing_id = remaining_after[remaining_after_idx];
                remaining_after_idx += 1;
                previous_id = existing_id;
                rebuilt_ids.push(existing_id);
                rebuilt_items.push(self.item_with_openhome_id(item.clone(), existing_id));
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    track_id = existing_id,
                    position = "AFTER pivot",
                    "KEPT existing track ID {}",
                    existing_id
                );
            } else {
                let metadata = build_metadata_xml(item);
                let new_id = self.playlist.insert(previous_id, &item.uri, &metadata)?;
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    after_id = previous_id,
                    new_id,
                    position = "AFTER pivot",
                    "RENDERER OP: Insert(after={}) -> new_id={}",
                    previous_id,
                    new_id
                );
                previous_id = new_id;
                rebuilt_ids.push(new_id);
                rebuilt_items.push(self.item_with_openhome_id(item.clone(), new_id));
            }
        }

        self.items = rebuilt_items;
        self.track_ids = rebuilt_ids;
        self.current_index = Some(pivot_idx_new); // Pivot is at its new position

        // VERIFICATION: Check that pivot ID is preserved
        let final_pivot_id = self.track_ids.get(pivot_idx_new).copied();
        if final_pivot_id != Some(pivot_id) {
            return Err(anyhow!(
                "CRITICAL BUG: Pivot ID changed from {} to {:?} during replace_queue_with_pivot!",
                pivot_id,
                final_pivot_id
            ));
        }

        debug!(
            renderer = self.renderer_id.0.as_str(),
            pivot_idx = pivot_idx_new,
            pivot_id,
            final_playlist_len = self.track_ids.len(),
            pivot_verified = true,
            "Gentle sync completed: double-LCS with pivot (playing track preserved)"
        );

        Ok(())
    }

    /// Standard LCS-based replacement (used when no currently playing item).
    fn replace_queue_standard_lcs(
        &mut self,
        items: Vec<PlaybackItem>,
        current_index: Option<usize>,
    ) -> Result<()> {
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

        // If we're replacing everything (keep=0), use delete_all() instead of
        // individual delete_id() calls. This is much more robust for live playlists
        // where track IDs can become invalid between refresh and deletion.
        if items_to_keep == 0 && items_to_delete > 0 {
            debug!(
                renderer = self.renderer_id.0.as_str(),
                "Using delete_all() for complete replacement (more robust for live playlists)"
            );
            self.playlist.delete_all()?;
            self.track_ids.clear();
            self.items.clear();
        } else {
            // Selective deletion when keeping some items
            for idx in (0..self.track_ids.len()).rev() {
                if !keep_current[idx] {
                    let track_id = self.track_ids[idx];
                    // Use delete_id_if_exists() to handle cases where another control point
                    // may have already modified the playlist
                    self.playlist.delete_id_if_exists(track_id)?;
                    self.track_ids.remove(idx);
                    self.items.remove(idx);
                }
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

        // Try to get the currently playing track ID from the renderer.
        // Note: Some OpenHome renderers (like upmpdcli) don't reliably support Info.Id(),
        // so we fall back to using our internal current_index pointer.
        let currently_playing_id_from_renderer = self
            .playlist.id().ok();

        // Find the currently playing item in our local state.
        // Priority: 1) Renderer-reported ID, 2) Our internal current_index
        let playing_info = if let Some(id) = currently_playing_id_from_renderer {
            // CASE: Renderer explicitly reported the playing track ID
            self.track_ids
                .iter()
                .position(|&tid| tid == id)
                .map(|idx| (idx, id, self.items[idx].uri.clone()))
        } else if let Some(idx) = self.current_index {
            // CASE: Use our internal pointer (fallback for renderers without Info.Id() support)
            if idx < self.track_ids.len() && idx < self.items.len() {
                let id = self.track_ids[idx];
                let uri = self.items[idx].uri.clone();
                debug!(
                    renderer = self.renderer_id.0.as_str(),
                    current_index = idx,
                    track_id = id,
                    "Using internal current_index as fallback (renderer didn't report playing ID)"
                );
                Some((idx, id, uri))
            } else {
                None
            }
        } else {
            None
        };

        debug!(
            renderer = self.renderer_id.0.as_str(),
            actual_items = self.items.len(),
            currently_playing_id_from_renderer = ?currently_playing_id_from_renderer,
            playing_info_detected = playing_info.is_some(),
            "OpenHome playlist state refreshed before replace_queue"
        );

        if let Some((playing_idx, playing_id, playing_uri)) = playing_info {
            // Find if the currently playing item is in the new playlist (by URI)
            let new_playing_idx = items.iter().position(|item| item.uri == playing_uri);

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

                self.replace_queue_preserve_current(items, playing_idx, playing_id)?;
            }
        } else {
            // No currently playing item or can't determine it - use standard LCS
            debug!(
                renderer = self.renderer_id.0.as_str(),
                "No currently playing item, using standard LCS sync"
            );
            self.replace_queue_standard_lcs(items, current_index)?;
        }

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

        // Use delete_id_if_exists() to handle cases where another control point
        // may have already modified the playlist
        self.playlist.delete_id_if_exists(track_id)?;
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
