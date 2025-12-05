use crate::model::TrackMetadata;
use crate::soap_client::{SoapCallResult, invoke_upnp_action};
use anyhow::{Result, anyhow};
use pmoupnp::soap::SoapEnvelope;
use xmltree::{Element, XMLNode};

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
        let args = [("aIdList", id_list_csv.as_str())];

        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "ReadList", &args)?;

        let envelope = ensure_success("ReadList", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "ReadListResponse")
            .ok_or_else(|| anyhow!("Missing ReadListResponse element in SOAP body"))?;

        let track_list_xml = extract_child_text(response, "aTrackList")?;
        parse_track_list(&track_list_xml)
    }

    pub fn insert(&self, after_id: u32, uri: &str, metadata: &str) -> Result<u32> {
        let after_id_str = after_id.to_string();
        let args = [
            ("aAfterId", after_id_str.as_str()),
            ("aUri", uri),
            ("aMetadata", metadata),
        ];

        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "Insert", &args)?;

        let envelope = ensure_success("Insert", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "InsertResponse")
            .ok_or_else(|| anyhow!("Missing InsertResponse element in SOAP body"))?;
        let new_id_text = extract_child_text(response, "aNewId")?;
        let new_id = new_id_text
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid aNewId value: {}", new_id_text))?;

        Ok(new_id)
    }

    pub fn play_id(&self, id: u32) -> Result<()> {
        let id_str = id.to_string();
        let args = [("aId", id_str.as_str())];

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
        let args = [("aSecond", second_str.as_str())];
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
        let args = [("aId", id_str.as_str())];

        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "DeleteId", &args)?;

        handle_action_response("DeleteId", &call_result)
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
        let value_text = extract_child_text(response, "aValue")?;
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

        let array_text = extract_child_text_any(response, &["aArray", "aIdArray"])?;
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
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        const MAX_BATCH: usize = 64;
        let mut entries = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(MAX_BATCH) {
            let mut batch = self.read_list(chunk)?;
            entries.append(&mut batch);
        }
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

        let uri = extract_child_text(response, "aUri")?;
        let metadata_xml = extract_child_text_optional(response, "aMetadata")
            .unwrap_or(None)
            .filter(|s| !s.is_empty());

        Ok(OhInfoTrack { uri, metadata_xml })
    }

    pub fn next(&self) -> Result<OhInfoTrack> {
        let call_result = invoke_upnp_action(&self.control_url, &self.service_type, "Next", &[])?;

        let envelope = ensure_success("Next", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "NextResponse")
            .ok_or_else(|| anyhow!("Missing NextResponse element in SOAP body"))?;

        let uri = extract_child_text(response, "aUri")?;
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
        let state = extract_child_text(response, "aState")?;
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

        let track_count = extract_child_text(response, "aTrackCount")?
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid aTrackCount value in Time response"))?;
        let duration_secs = extract_child_text(response, "aDuration")?
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid aDuration value in Time response"))?;
        let elapsed_secs = extract_child_text(response, "aSeconds")?
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
        let value = extract_child_text(response, "aVolume")?;
        let parsed = value
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid volume value: {}", value))?;
        Ok(parsed.min(u16::MAX as u32) as u16)
    }

    pub fn set_volume(&self, vol: u16) -> Result<()> {
        let vol_str = vol.to_string();
        let args = [("aVolume", vol_str.as_str())];
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "SetVolume", &args)?;
        handle_action_response("SetVolume", &call_result)
    }

    pub fn mute(&self) -> Result<bool> {
        let call_result = invoke_upnp_action(&self.control_url, &self.service_type, "Mute", &[])?;
        let envelope = ensure_success("Mute", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "MuteResponse")
            .ok_or_else(|| anyhow!("Missing MuteResponse element in SOAP body"))?;
        let value = extract_child_text(response, "aMute")?;
        parse_bool(&value)
    }

    pub fn set_mute(&self, mute: bool) -> Result<()> {
        let mute_str = if mute { "1" } else { "0" };
        let args = [("aMute", mute_str)];
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
        let args = [("aId", id_str.as_str())];
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "PlayChannel", &args)?;
        handle_action_response("PlayChannel", &call_result)
    }

    pub fn channel(&self, id: u32) -> Result<OhRadioChannel> {
        let id_str = id.to_string();
        let args = [("aId", id_str.as_str())];
        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "Channel", &args)?;

        let envelope = ensure_success("Channel", &call_result)?;
        let response = find_child_with_suffix(&envelope.body.content, "ChannelResponse")
            .ok_or_else(|| anyhow!("Missing ChannelResponse element in SOAP body"))?;

        let uri = extract_child_text(response, "aUri")?;
        let metadata_xml = extract_child_text_optional(response, "aMetadata")
            .unwrap_or(None)
            .filter(|s| !s.is_empty());

        Ok(OhRadioChannel { uri, metadata_xml })
    }
}

pub fn parse_track_metadata_from_didl(xml: &str) -> Option<TrackMetadata> {
    if xml.trim().is_empty() {
        return None;
    }

    let parsed = pmodidl::parse_metadata::<pmodidl::DIDLLite>(xml).ok()?;
    let item = parsed.data.items.first()?;

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

fn parse_track_list(xml: &str) -> Result<Vec<OhTrackEntry>> {
    if xml.trim().is_empty() {
        return Ok(Vec::new());
    }

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
    let id_text = extract_child_text(elem, "Id")?;
    let id = id_text
        .parse::<u32>()
        .map_err(|_| anyhow!("Invalid OpenHome Entry Id: {}", id_text))?;
    let uri = extract_child_text(elem, "Uri")?;
    let metadata_xml = extract_child_text_optional(elem, "Metadata")?.unwrap_or_default();

    Ok(OhTrackEntry {
        id,
        uri,
        metadata_xml,
    })
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

#[derive(Debug, Clone)]
struct UpnpError {
    pub error_code: u32,
    pub error_description: String,
}

fn parse_upnp_error(envelope: &SoapEnvelope) -> Option<UpnpError> {
    let fault = find_child_with_suffix(&envelope.body.content, "Fault")?;
    let detail = find_child_with_suffix(fault, "detail")?;
    let upnp_error = find_child_with_suffix(detail, "UPnPError")?;

    let error_code_elem = upnp_error.children.iter().find_map(|node| match node {
        XMLNode::Element(elem) if elem.name.ends_with("errorCode") => Some(elem),
        _ => None,
    })?;

    let error_code_text = error_code_elem.get_text()?.trim().to_string();
    let error_code = error_code_text.parse::<u32>().ok()?;

    let error_description = upnp_error
        .children
        .iter()
        .find_map(|node| match node {
            XMLNode::Element(elem) if elem.name.ends_with("errorDescription") => {
                elem.get_text().map(|t| t.trim().to_string())
            }
            _ => None,
        })
        .unwrap_or_default();

    Some(UpnpError {
        error_code,
        error_description,
    })
}

fn find_child_with_suffix<'a>(parent: &'a Element, suffix: &str) -> Option<&'a Element> {
    parent.children.iter().find_map(|node| match node {
        XMLNode::Element(elem) if elem.name.ends_with(suffix) => Some(elem),
        _ => None,
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
