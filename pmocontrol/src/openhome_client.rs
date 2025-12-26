use crate::model::TrackMetadata;
use crate::soap_client::{invoke_upnp_action, parse_upnp_error, SoapCallResult};
use anyhow::{anyhow, Result};
use pmoupnp::soap::SoapEnvelope;
use tracing::{debug, info, warn};
use xmltree::{Element, XMLNode};

/// Value used by OpenHome renderers to indicate "insert at the head".
/// Several implementations, including upmpdcli, expect zero rather than the
/// historical 0xFFFFFFFF sentinel.
pub const OPENHOME_PLAYLIST_HEAD_ID: u32 = 0;

#[derive(Debug, Clone)]
pub struct OhTrackEntry {
    pub id: u32,
    pub uri: String,
    pub metadata_xml: String,
}

#[derive(Debug, Clone)]
pub struct OhInfoTrack {
    pub uri: String,
    pub metadata_xml: Option<String>,
}

impl OhInfoTrack {
    pub fn metadata(&self) -> Option<TrackMetadata> {
        self.metadata_xml
            .as_deref()
            .and_then(parse_track_metadata_from_didl)
    }
}

#[derive(Debug, Clone)]
pub struct OhTimePosition {
    pub track_count: u32,
    pub duration_secs: u32,
    pub elapsed_secs: u32,
}

#[derive(Debug, Clone)]
pub struct OhRadioChannel {
    pub uri: String,
    pub metadata_xml: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OhProductSource {
    pub name: String,
    pub source_type: String,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct OhPlaylistClient {
    pub control_url: String,
    pub service_type: String,
}

impl OhPlaylistClient {
    pub fn new(control_url: String, service_type: String) -> Self {
        Self {
            control_url,
            service_type,
        }
    }

    pub fn read_list(&self, id_list: &[u32]) -> Result<Vec<OhTrackEntry>> {
        if id_list.is_empty() {
            return Ok(Vec::new());
        }

        let id_list_csv = id_list
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let args = [("IdList", id_list_csv.as_str())];

        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "ReadList", &args)?;

        let envelope = ensure_success("ReadList", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "ReadListResponse")
            .ok_or_else(|| anyhow!("Missing ReadListResponse element in SOAP body"))?;

        let track_list_b64 =
            extract_child_text_any(response, &["aTrackList", "TrackList", "aValue", "Value"])?;
        let track_list_sample: String = track_list_b64.chars().take(256).collect();
        debug!(
            control_url = self.control_url.as_str(),
            track_list_len = track_list_b64.len(),
            track_list_sample = %track_list_sample,
            "OpenHome ReadList returned raw TrackList content"
        );
        parse_track_list(&track_list_b64)
    }

    pub fn insert(&self, after_id: u32, uri: &str, metadata: &str) -> Result<u32> {
        let after_id_str = after_id.to_string();
        let args = [
            ("AfterId", after_id_str.as_str()),
            ("Uri", uri),
            ("Metadata", metadata),
        ];

        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "Insert", &args)?;

        let envelope = ensure_success("Insert", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "InsertResponse")
            .ok_or_else(|| anyhow!("Missing InsertResponse element in SOAP body"))?;
        let new_id_text =
            extract_child_text_any(response, &["aNewId", "NewId", "aValue", "Value"])?;
        let new_id = new_id_text
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid aNewId value: {}", new_id_text))?;

        Ok(new_id)
    }

    pub fn play_id(&self, id: u32) -> Result<()> {
        let id_str = id.to_string();
        let args = [("Id", id_str.as_str())];

        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "PlayId", &args)?;

        handle_action_response("PlayId", &call_result)
    }

    pub fn play(&self) -> Result<()> {
        let call_result = invoke_upnp_action(&self.control_url, &self.service_type, "Play", &[])?;
        handle_action_response("Play", &call_result)
    }

    pub fn pause(&self) -> Result<()> {
        let call_result = invoke_upnp_action(&self.control_url, &self.service_type, "Pause", &[])?;
        handle_action_response("Pause", &call_result)
    }

    pub fn stop(&self) -> Result<()> {
        let call_result = invoke_upnp_action(&self.control_url, &self.service_type, "Stop", &[])?;
        handle_action_response("Stop", &call_result)
    }

    pub fn next(&self) -> Result<()> {
        let call_result = invoke_upnp_action(&self.control_url, &self.service_type, "Next", &[])?;
        handle_action_response("Next", &call_result)
    }

    pub fn previous(&self) -> Result<()> {
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "Previous", &[])?;
        handle_action_response("Previous", &call_result)
    }

    pub fn seek_second_absolute(&self, second: u32) -> Result<()> {
        let second_str = second.to_string();
        let args = [("Second", second_str.as_str())];
        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "SeekSecondAbsolute",
            &args,
        )?;

        handle_action_response("SeekSecondAbsolute", &call_result)
    }

    pub fn delete_id(&self, id: u32) -> Result<()> {
        let id_str = id.to_string();
        let args = [("Id", id_str.as_str())];

        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "DeleteId", &args)?;

        handle_action_response("DeleteId", &call_result)
    }

    /// Attempts to delete an OpenHome playlist entry by ID.
    /// Unlike delete_id(), this function silently ignores errors related to invalid/missing IDs,
    /// which is useful in multi-control-point scenarios where playlist state may have changed.
    ///
    /// Returns:
    /// - Ok(true) if the ID was successfully deleted
    /// - Ok(false) if the ID didn't exist (logged as warning)
    /// - Err(_) for other errors (network issues, etc.)
    pub fn delete_id_if_exists(&self, id: u32) -> Result<bool> {
        match self.delete_id(id) {
            Ok(()) => Ok(true),
            Err(err) => {
                // Check if this is an error about an invalid/missing ID
                // OpenHome servers may return different error messages/codes for this case
                let err_msg = format!("{err}");
                if err_msg.contains("Invalid")
                    || err_msg.contains("invalid")
                    || err_msg.contains("not found")
                    || err_msg.contains("does not exist")
                    || err_msg.contains("unknown")
                    || err_msg.contains("500") // HTTP 500 = renderer in inconsistent state
                    || err_msg.contains("Action Failed") // UPnP error 501
                {
                    warn!(
                        control_url = self.control_url.as_str(),
                        id,
                        "DeleteId silently ignored - ID does not exist or renderer in inconsistent state (likely modified by events)"
                    );
                    Ok(false)
                } else {
                    // Re-throw other errors (network issues, etc.)
                    Err(err)
                }
            }
        }
    }

    pub fn delete_all(&self) -> Result<()> {
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "DeleteAll", &[])?;
        handle_action_response("DeleteAll", &call_result)
    }

    pub fn tracks_max(&self) -> Result<u32> {
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "TracksMax", &[])?;

        let envelope = ensure_success("TracksMax", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "TracksMaxResponse")
            .ok_or_else(|| anyhow!("Missing TracksMaxResponse element in SOAP body"))?;
        let value_text = extract_child_text_any(response, &["aValue", "Value"])?;
        let value = value_text
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid TracksMax value: {}", value_text))?;

        Ok(value)
    }

    pub fn id_array(&self) -> Result<Vec<u32>> {
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "IdArray", &[])?;
        let envelope = ensure_success("IdArray", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "IdArrayResponse")
            .ok_or_else(|| anyhow!("Missing IdArrayResponse element in SOAP body"))?;

        // Try to extract the array element. If missing, assume empty playlist.
        let array_text = match extract_child_text_any(
            response,
            &["aArray", "Array", "aIdArray", "IdArray", "aValue", "Value"],
        ) {
            Ok(text) => text,
            Err(_) => {
                // Element not found - playlist is likely empty
                return Ok(Vec::new());
            }
        };

        // Handle empty string (another way renderers indicate empty playlist)
        if array_text.trim().is_empty() {
            return Ok(Vec::new());
        }

        let bytes = decode_base64(&array_text)?;
        if bytes.len() % 4 != 0 {
            return Err(anyhow!(
                "Invalid IdArray payload length {} (expected multiple of 4)",
                bytes.len()
            ));
        }

        let mut ids = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            ids.push(u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        Ok(ids)
    }

    pub fn read_all_tracks(&self) -> Result<Vec<OhTrackEntry>> {
        let ids = self.id_array()?;
        debug!(
            control_url = self.control_url.as_str(),
            id_count = ids.len(),
            "OpenHome Playlist IdArray returned"
        );

        if ids.is_empty() {
            info!(
                control_url = self.control_url.as_str(),
                "OpenHome Playlist is empty (no track IDs)"
            );
            return Ok(Vec::new());
        }

        const MAX_BATCH: usize = 64;
        let mut entries = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(MAX_BATCH) {
            match self.read_list(chunk) {
                Ok(mut batch) => entries.append(&mut batch),
                Err(err) if chunk.len() > 1 && is_invalid_entry_id_error(&err) => {
                    debug!(
                        control_url = self.control_url.as_str(),
                        requested = chunk.len(),
                        "ReadList chunk failed with invalid entry ids, falling back to per-id requests"
                    );
                    for id in chunk {
                        match self.read_list(&[*id]) {
                            Ok(mut single) => entries.append(&mut single),
                            Err(inner_err) => return Err(inner_err),
                        }
                    }
                }
                Err(err) => return Err(err),
            }
        }

        debug!(
            control_url = self.control_url.as_str(),
            track_count = entries.len(),
            expected_count = ids.len(),
            "OpenHome Playlist tracks read"
        );

        Ok(entries)
    }
}

#[derive(Debug, Clone)]
pub struct OhInfoClient {
    pub control_url: String,
    pub service_type: String,
}

impl OhInfoClient {
    pub fn new(control_url: String, service_type: String) -> Self {
        Self {
            control_url,
            service_type,
        }
    }

    pub fn track(&self) -> Result<OhInfoTrack> {
        let call_result = invoke_upnp_action(&self.control_url, &self.service_type, "Track", &[])?;

        let envelope = ensure_success("Track", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "TrackResponse")
            .ok_or_else(|| anyhow!("Missing TrackResponse element in SOAP body"))?;

        let uri = extract_child_text_any(response, &["aUri", "Uri", "aValue", "Value"])
            .unwrap_or_default();
        let metadata_xml = extract_child_text_optional(response, "aMetadata")
            .unwrap_or(None)
            .filter(|s| !s.is_empty());

        debug!(
            uri = uri.as_str(),
            has_metadata = metadata_xml.is_some(),
            metadata_length = metadata_xml.as_ref().map(|s| s.len()),
            "OpenHome Info.Track() returned"
        );

        if let Some(ref xml) = metadata_xml {
            debug!(metadata_xml = xml.as_str(), "OpenHome metadata XML content");
        }

        Ok(OhInfoTrack { uri, metadata_xml })
    }

    pub fn next(&self) -> Result<OhInfoTrack> {
        let call_result = invoke_upnp_action(&self.control_url, &self.service_type, "Next", &[])?;

        let envelope = ensure_success("Next", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "NextResponse")
            .ok_or_else(|| anyhow!("Missing NextResponse element in SOAP body"))?;

        let uri = extract_child_text_any(response, &["aUri", "Uri", "aValue", "Value"])
            .unwrap_or_default();
        let metadata_xml = extract_child_text_optional(response, "aMetadata")
            .unwrap_or(None)
            .filter(|s| !s.is_empty());

        Ok(OhInfoTrack { uri, metadata_xml })
    }

    pub fn id(&self) -> Result<u32> {
        let call_result = invoke_upnp_action(&self.control_url, &self.service_type, "Id", &[])?;

        let envelope = ensure_success("Id", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "IdResponse")
            .ok_or_else(|| anyhow!("Missing IdResponse element in SOAP body"))?;
        let id_text = extract_child_text(response, "aId")?;
        let id = id_text
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid Info.Id value: {}", id_text))?;
        Ok(id)
    }

    pub fn transport_state(&self) -> Result<String> {
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "TransportState", &[])?;

        let envelope = ensure_success("TransportState", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "TransportStateResponse")
            .ok_or_else(|| anyhow!("Missing TransportStateResponse element in SOAP body"))?;
        let state = extract_child_text_any(response, &["aState", "State", "aValue", "Value"])?;
        Ok(state)
    }

    pub fn read_current_metadata(&self) -> Result<Option<TrackMetadata>> {
        let track = self.track()?;
        Ok(track.metadata())
    }
}

#[derive(Debug, Clone)]
pub struct OhTimeClient {
    pub control_url: String,
    pub service_type: String,
}

impl OhTimeClient {
    pub fn new(control_url: String, service_type: String) -> Self {
        Self {
            control_url,
            service_type,
        }
    }

    pub fn position(&self) -> Result<OhTimePosition> {
        let call_result = invoke_upnp_action(&self.control_url, &self.service_type, "Time", &[])?;

        let envelope = ensure_success("Time", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "TimeResponse")
            .ok_or_else(|| anyhow!("Missing TimeResponse element in SOAP body"))?;

        let track_count =
            extract_child_text_any(response, &["aTrackCount", "TrackCount", "aValue", "Value"])?
                .parse::<u32>()
                .map_err(|_| anyhow!("Invalid aTrackCount value in Time response"))?;
        let duration_secs =
            extract_child_text_any(response, &["aDuration", "Duration", "aValue", "Value"])?
                .parse::<u32>()
                .map_err(|_| anyhow!("Invalid aDuration value in Time response"))?;
        let elapsed_secs =
            extract_child_text_any(response, &["aSeconds", "Seconds", "aValue", "Value"])?
                .parse::<u32>()
                .map_err(|_| anyhow!("Invalid aSeconds value in Time response"))?;

        Ok(OhTimePosition {
            track_count,
            duration_secs,
            elapsed_secs,
        })
    }
}

#[derive(Debug, Clone)]
pub struct OhVolumeClient {
    pub control_url: String,
    pub service_type: String,
}

impl OhVolumeClient {
    pub fn new(control_url: String, service_type: String) -> Self {
        Self {
            control_url,
            service_type,
        }
    }

    pub fn volume(&self) -> Result<u16> {
        let call_result = invoke_upnp_action(&self.control_url, &self.service_type, "Volume", &[])?;
        let envelope = ensure_success("Volume", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "VolumeResponse")
            .ok_or_else(|| anyhow!("Missing VolumeResponse element in SOAP body"))?;
        let value = extract_child_text_any(response, &["aVolume", "Volume", "aValue", "Value"])?;
        let parsed = value
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid volume value: {}", value))?;
        Ok(parsed.min(u16::MAX as u32) as u16)
    }

    pub fn set_volume(&self, vol: u16) -> Result<()> {
        let vol_str = vol.to_string();
        let args = [("Value", vol_str.as_str())];
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "SetVolume", &args)?;
        handle_action_response("SetVolume", &call_result)
    }

    pub fn mute(&self) -> Result<bool> {
        let call_result = invoke_upnp_action(&self.control_url, &self.service_type, "Mute", &[])?;
        let envelope = ensure_success("Mute", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "MuteResponse")
            .ok_or_else(|| anyhow!("Missing MuteResponse element in SOAP body"))?;
        let value = extract_child_text_any(response, &["aMute", "Mute", "aValue", "Value"])?;
        parse_bool(&value)
    }

    pub fn set_mute(&self, mute: bool) -> Result<()> {
        let mute_str = if mute { "1" } else { "0" };
        let args = [("Mute", mute_str)];
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "SetMute", &args)?;
        handle_action_response("SetMute", &call_result)
    }
}

#[derive(Debug, Clone)]
pub struct OhRadioClient {
    pub control_url: String,
    pub service_type: String,
}

impl OhRadioClient {
    pub fn new(control_url: String, service_type: String) -> Self {
        Self {
            control_url,
            service_type,
        }
    }

    pub fn play_channel(&self, id: u32) -> Result<()> {
        let id_str = id.to_string();
        let args = [("Id", id_str.as_str())];
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "PlayChannel", &args)?;
        handle_action_response("PlayChannel", &call_result)
    }

    pub fn channel(&self, id: u32) -> Result<OhRadioChannel> {
        let id_str = id.to_string();
        let args = [("Id", id_str.as_str())];
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "Channel", &args)?;

        let envelope = ensure_success("Channel", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "ChannelResponse")
            .ok_or_else(|| anyhow!("Missing ChannelResponse element in SOAP body"))?;

        let uri = extract_child_text_any(response, &["aUri", "Uri", "aValue", "Value"])
            .unwrap_or_default();
        let metadata_xml = extract_child_text_optional(response, "aMetadata")
            .unwrap_or(None)
            .filter(|s| !s.is_empty());

        Ok(OhRadioChannel { uri, metadata_xml })
    }
}

#[derive(Debug, Clone)]
pub struct OhProductClient {
    pub control_url: String,
    pub service_type: String,
}

impl OhProductClient {
    pub fn new(control_url: String, service_type: String) -> Self {
        Self {
            control_url,
            service_type,
        }
    }

    pub fn source_xml(&self) -> Result<Vec<OhProductSource>> {
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "SourceXml", &[])?;
        let envelope = ensure_success("SourceXml", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "SourceXmlResponse")
            .ok_or_else(|| anyhow!("Missing SourceXmlResponse element in SOAP body"))?;
        let xml = extract_child_text_any(response, &["aSourceXml", "aXml", "aValue", "Value"])?;
        parse_product_source_list(&xml)
    }

    pub fn source_index(&self) -> Result<u32> {
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "SourceIndex", &[])?;
        let envelope = ensure_success("SourceIndex", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "SourceIndexResponse")
            .ok_or_else(|| anyhow!("Missing SourceIndexResponse element in SOAP body"))?;
        let value = extract_child_text_any(response, &["aIndex", "Index", "aValue", "Value"])?;
        value
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid Product.SourceIndex value: {}", value))
    }

    pub fn set_source_index(&self, index: u32) -> Result<()> {
        let value = index.to_string();
        let args = [("Index", value.as_str())];
        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "SetSourceIndex",
            &args,
        )?;
        handle_action_response("SetSourceIndex", &call_result)
    }

    pub fn ensure_playlist_source_selected(&self) -> Result<()> {
        let sources = self.source_xml()?;

        // Log all available sources for diagnostics
        debug!(
            control_url = self.control_url.as_str(),
            source_count = sources.len(),
            "OpenHome Product sources available"
        );
        for (idx, source) in sources.iter().enumerate() {
            debug!(
                control_url = self.control_url.as_str(),
                index = idx,
                name = source.name.as_str(),
                source_type = source.source_type.as_str(),
                visible = source.visible,
                "OpenHome source"
            );
        }

        let playlist_index = sources
            .iter()
            .position(|source| source.source_type.eq_ignore_ascii_case("playlist"))
            .ok_or_else(|| {
                warn!(
                    control_url = self.control_url.as_str(),
                    available_types = ?sources.iter().map(|s| s.source_type.as_str()).collect::<Vec<_>>(),
                    "OpenHome Product source list does not expose a Playlist entry"
                );
                anyhow!("OpenHome Product source list does not expose a Playlist entry")
            })?;
        let playlist_index = playlist_index as u32;
        let current_index = self.source_index()?;

        // Log current source state
        let current_source = sources.get(current_index as usize);
        debug!(
            control_url = self.control_url.as_str(),
            current_index,
            current_source_name = current_source.map(|s| s.name.as_str()).unwrap_or("unknown"),
            current_source_type = current_source.map(|s| s.source_type.as_str()).unwrap_or("unknown"),
            playlist_index,
            needs_switch = current_index != playlist_index,
            "OpenHome source state"
        );

        if current_index != playlist_index {
            info!(
                control_url = self.control_url.as_str(),
                from_index = current_index,
                to_index = playlist_index,
                "Switching OpenHome Product source to Playlist"
            );
            self.set_source_index(playlist_index)?;

            // Verify the switch was successful
            let new_index = self.source_index()?;
            if new_index == playlist_index {
                info!(
                    control_url = self.control_url.as_str(),
                    "Successfully switched to Playlist source"
                );
            } else {
                warn!(
                    control_url = self.control_url.as_str(),
                    expected = playlist_index,
                    actual = new_index,
                    "Source switch may have failed - index mismatch"
                );
            }
        }
        Ok(())
    }
}

pub fn parse_track_metadata_from_didl(xml: &str) -> Option<TrackMetadata> {
    if xml.trim().is_empty() {
        return None;
    }

    let parsed = pmodidl::parse_metadata::<pmodidl::DIDLLite>(xml).ok()?;
    let item = parsed.data.items.first()?;

    debug!(
        title = item.title.as_str(),
        has_album_art = item.album_art.is_some(),
        album_art_uri = item.album_art.as_deref(),
        "Parsed DIDL metadata for track"
    );

    Some(TrackMetadata {
        title: Some(item.title.clone()),
        artist: item.artist.clone(),
        album: item.album.clone(),
        genre: item.genre.clone(),
        album_art_uri: item.album_art.clone(),
        date: item.date.clone(),
        track_number: item.original_track_number.clone(),
        creator: item.creator.clone(),
    })
}

fn parse_track_list(payload: &str) -> Result<Vec<OhTrackEntry>> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let (xml, was_base64) = if trimmed.starts_with('<') {
        (trimmed.to_string(), false)
    } else {
        let bytes = decode_base64(trimmed)?;
        let decoded = String::from_utf8(bytes)
            .map_err(|err| anyhow!("TrackList payload not valid UTF-8 after base64 decode: {err}"))?;
        (decoded, true)
    };
    let xml_sample: String = xml.chars().take(256).collect();
    debug!(
        raw_base64 = was_base64,
        decoded_len = xml.len(),
        decoded_sample = %xml_sample,
        "Decoded OpenHome TrackList payload"
    );

    let mut reader = std::io::Cursor::new(xml.as_bytes());
    let root = Element::parse(&mut reader)
        .map_err(|err| anyhow!("Failed to parse OpenHome TrackList XML: {}", err))?;
    let mut entries = Vec::new();

    for node in &root.children {
        if let XMLNode::Element(elem) = node {
            if elem.name.ends_with("Entry") {
                entries.push(parse_track_entry(elem)?);
            }
        }
    }

    Ok(entries)
}

fn parse_track_entry(elem: &Element) -> Result<OhTrackEntry> {
    let id_text = extract_child_text_local(elem, "Id")?;

    // Some renderers (like upmpdcli) return a comma-separated list of IDs in the Id element
    // when using ReadList. This is a non-standard compact format that we cannot parse properly
    // because we need to fetch each track individually to get its Uri and Metadata.
    // Return an error to force the fallback to individual Read() calls.
    if id_text.contains(',') {
        debug!(
            raw_entry = %elem.name,
            raw_id = id_text.as_str(),
            "Multi-value Id element detected - forcing fallback to individual reads"
        );
        return Err(anyhow!(
            "Renderer returned comma-separated IDs in single Entry - need individual reads"
        ));
    }

    let id = id_text
        .parse::<u32>()
        .map_err(|_| anyhow!("Invalid OpenHome Entry Id: {}", id_text))?;

    let uri = extract_child_text_local(elem, "Uri")?;
    let metadata_xml = extract_child_text_optional_local(elem, "Metadata")?.unwrap_or_default();

    Ok(OhTrackEntry {
        id,
        uri,
        metadata_xml,
    })
}

fn parse_product_source_list(xml: &str) -> Result<Vec<OhProductSource>> {
    if xml.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut reader = std::io::Cursor::new(xml.as_bytes());
    let root = Element::parse(&mut reader)
        .map_err(|err| anyhow!("Failed to parse OpenHome SourceXml payload: {}", err))?;

    let mut sources = Vec::new();
    for node in &root.children {
        if let XMLNode::Element(elem) = node {
            if elem.name.ends_with("Source") {
                let name = extract_child_text(elem, "Name")?;
                let source_type = extract_child_text(elem, "Type")?;
                let visible = extract_child_text_optional(elem, "Visible")?
                    .map(|v| parse_visible_flag(&v))
                    .unwrap_or(true);

                sources.push(OhProductSource {
                    name,
                    source_type,
                    visible,
                });
            }
        }
    }

    Ok(sources)
}

fn parse_visible_flag(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("true") {
        return true;
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return false;
    }
    trimmed == "1"
}

fn ensure_success<'a>(action: &str, call_result: &'a SoapCallResult) -> Result<&'a SoapEnvelope> {
    if !call_result.status.is_success() {
        if let Some(env) = &call_result.envelope {
            if let Some(err) = parse_upnp_error(env) {
                return Err(anyhow!(
                    "{action} failed with UPnP error {}: {} (HTTP status {})",
                    err.error_code,
                    err.error_description,
                    call_result.status
                ));
            }
        }

        return Err(anyhow!(
            "{action} failed with HTTP status {} and body: {}",
            call_result.status,
            call_result.raw_body
        ));
    }

    let envelope = call_result
        .envelope
        .as_ref()
        .ok_or_else(|| anyhow!("Missing SOAP envelope in {action} response"))?;

    if let Some(err) = parse_upnp_error(envelope) {
        return Err(anyhow!(
            "{action} returned UPnP error {}: {} (HTTP status {})",
            err.error_code,
            err.error_description,
            call_result.status
        ));
    }

    Ok(envelope)
}

fn handle_action_response(action: &str, call_result: &SoapCallResult) -> Result<()> {
    ensure_success(action, call_result)?;
    Ok(())
}

fn find_child_with_suffix<'a>(parent: &'a Element, suffix: &str) -> Option<&'a Element> {
    parent.children.iter().find_map(|node| match node {
        XMLNode::Element(elem) if elem.name.ends_with(suffix) => Some(elem),
        _ => None,
    })
}

fn find_child_with_local_name<'a>(parent: &'a Element, local: &str) -> Option<&'a Element> {
    parent.children.iter().find_map(|node| {
        if let XMLNode::Element(elem) = node {
            if elem.name == local || elem.name.ends_with(&format!(":{}", local)) {
                return Some(elem);
            }
        }
        None
    })
}

fn extract_child_text(parent: &Element, suffix: &str) -> Result<String> {
    let child = find_child_with_suffix(parent, suffix)
        .ok_or_else(|| anyhow!("Missing {suffix} element in response"))?;

    let text = child
        .get_text()
        .map(|t| t.trim().to_string())
        .ok_or_else(|| anyhow!("{suffix} element missing text in response"))?;

    Ok(text)
}

fn extract_child_text_optional(parent: &Element, suffix: &str) -> Result<Option<String>> {
    if let Some(child) = find_child_with_suffix(parent, suffix) {
        let text = child
            .get_text()
            .map(|t| t.trim().to_string())
            .unwrap_or_default();
        Ok(Some(text))
    } else {
        Ok(None)
    }
}

fn extract_child_text_any(parent: &Element, suffixes: &[&str]) -> Result<String> {
    for suffix in suffixes {
        if let Ok(text) = extract_child_text(parent, suffix) {
            return Ok(text);
        }
    }
    Err(anyhow!(
        "Missing {} element in response",
        suffixes.join(" or ")
    ))
}

fn extract_child_text_local(parent: &Element, local: &str) -> Result<String> {
    let child = find_child_with_local_name(parent, local)
        .ok_or_else(|| anyhow!("Missing {local} element in response"))?;
    let text = child
        .get_text()
        .map(|t| t.trim().to_string())
        .ok_or_else(|| anyhow!("{local} element missing text in response"))?;
    Ok(text)
}

fn extract_child_text_optional_local(parent: &Element, local: &str) -> Result<Option<String>> {
    if let Some(child) = find_child_with_local_name(parent, local) {
        let text = child
            .get_text()
            .map(|t| t.trim().to_string())
            .unwrap_or_default();
        Ok(Some(text))
    } else {
        Ok(None)
    }
}

fn parse_bool(value: &str) -> Result<bool> {
    match value.trim() {
        "0" => Ok(false),
        "1" => Ok(true),
        other => Err(anyhow!("Invalid boolean value '{}'", other)),
    }
}

pub(crate) fn decode_base64(input: &str) -> Result<Vec<u8>> {
    fn value(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let mut output = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits_collected: u8 = 0;

    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        if byte == b'\r' || byte == b'\n' || byte == b' ' || byte == b'\t' {
            continue;
        }
        let val =
            value(byte).ok_or_else(|| anyhow!("Invalid base64 character '{}'", byte as char))?;
        buffer = (buffer << 6) | (val as u32);
        bits_collected += 6;
        if bits_collected >= 8 {
            bits_collected -= 8;
            let out = (buffer >> bits_collected) & 0xFF;
            output.push(out as u8);
        }
    }

    Ok(output)
}

fn is_invalid_entry_id_error(err: &anyhow::Error) -> bool {
    let msg = format!("{err}");
    msg.contains("Invalid OpenHome Entry Id") || msg.contains("comma-separated IDs")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parse_insert_response_accepts_newid_without_prefix() {
        let xml = r#"<u:InsertResponse xmlns:u="urn:av-openhome-org:service:Playlist:1"><NewId>1337</NewId></u:InsertResponse>"#;
        let mut cursor = Cursor::new(xml.as_bytes());
        let response = Element::parse(&mut cursor).expect("valid xml");
        let value =
            extract_child_text_any(&response, &["aNewId", "NewId", "aValue", "Value"]).unwrap();
        assert_eq!(value, "1337");
    }

    #[test]
    fn parse_readlist_response_accepts_tracklist_without_prefix() {
        let xml = r#"<u:ReadListResponse xmlns:u="urn:av-openhome-org:service:Playlist:1"><TrackList>PGVudHJ5PjwvZW50cnk+</TrackList></u:ReadListResponse>"#;
        let mut cursor = Cursor::new(xml.as_bytes());
        let response = Element::parse(&mut cursor).expect("valid xml");
        let value =
            extract_child_text_any(&response, &["aTrackList", "TrackList", "aValue", "Value"])
                .expect("tracklist");
        assert_eq!(value, "PGVudHJ5PjwvZW50cnk+");
    }

    #[test]
    fn parse_idarray_response_accepts_array_without_prefix() {
        let xml = r#"<u:IdArrayResponse xmlns:u="urn:av-openhome-org:service:Playlist:1"><Token>1</Token><Array>AAAAAQAAAAI=</Array></u:IdArrayResponse>"#;
        let mut cursor = Cursor::new(xml.as_bytes());
        let response = Element::parse(&mut cursor).expect("valid xml");
        let value = extract_child_text_any(
            &response,
            &["aArray", "Array", "aIdArray", "IdArray", "aValue", "Value"],
        )
        .expect("array content");
        assert_eq!(value, "AAAAAQAAAAI=");
    }
}
