use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use pmodidl::DIDLLite;
use pmoupnp::soap::SoapEnvelope;
use pmoupnp::soap::error_codes;
use tracing::{debug, warn};
use xmltree::{Element, XMLNode};

use crate::errors::ControlPointError;
use crate::model::TrackMetadata;
use crate::online::{DeviceConnectionState, DeviceOnline};
use crate::queue::PlaybackItem;
use crate::soap_client::{SoapCallResult, invoke_upnp_action_with_timeout};
use crate::{DEFAULT_HTTP_TIMEOUT, DeviceId, DeviceIdentity};

/// Snapshot of a media server discovered through UPnP SSDP.
#[derive(Clone, Debug)]
pub struct UpnpMediaServer {
    id: DeviceId,
    udn: String,
    friendly_name: String,
    model_name: String,
    manufacturer: String,
    location: String,
    server_header: String,
    has_content_directory: bool,
    content_directory_service_type: Option<String>,
    content_directory_control_url: Option<String>,
    connection: Arc<Mutex<DeviceConnectionState>>,
}

impl UpnpMediaServer {
    pub fn new(
        id: DeviceId,
        udn: String,
        friendly_name: String,
        model_name: String,
        manufacturer: String,
        location: String,
        server_header: String,
        has_content_directory: bool,
        content_directory_service_type: Option<String>,
        content_directory_control_url: Option<String>,
    ) -> Self {
        Self {
            id,
            udn,
            friendly_name,
            model_name,
            manufacturer,
            location,
            server_header,
            has_content_directory,
            content_directory_service_type,
            content_directory_control_url,
            connection: DeviceConnectionState::make(),
        }
    }

    pub fn make(
        id: DeviceId,
        udn: String,
        friendly_name: String,
        model_name: String,
        manufacturer: String,
        location: String,
        server_header: String,
        has_content_directory: bool,
        content_directory_service_type: Option<String>,
        content_directory_control_url: Option<String>,
    ) -> Arc<Self> {
        Arc::new(UpnpMediaServer::new(
            id,
            udn,
            friendly_name,
            model_name,
            manufacturer,
            location,
            server_header,
            has_content_directory,
            content_directory_service_type,
            content_directory_control_url,
        ))
    }

    fn browse_with_flag(
        &self,
        object_id: &str,
        browse_flag: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<MediaEntry>, ControlPointError> {
        let start_str = start.to_string();
        let count_str = count.to_string();
        let args = vec![
            ("ObjectID", object_id.to_string()),
            ("BrowseFlag", browse_flag.to_string()),
            ("Filter", "*".to_string()),
            ("StartingIndex", start_str),
            ("RequestedCount", count_str),
            ("SortCriteria", String::new()),
        ];

        let response = self.invoke_content_directory("Browse", None, args)?;
        let envelope = response.envelope.ok_or_else(|| {
            ControlPointError::MediaServerError(format!("Missing SOAP envelope in Browse response"))
        })?;
        let didl_xml = extract_result_payload(&envelope, "BrowseResponse")?;
        map_didl_entries(&didl_xml)
    }

    fn has_content_directory(&self) -> bool {
        self.has_content_directory
    }

    fn search_impl(
        &self,
        container_id: &str,
        query: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<MediaEntry>, ControlPointError> {
        let start_str = start.to_string();
        let count_str = count.to_string();

        let args = vec![
            ("ContainerID", container_id.to_string()),
            ("SearchCriteria", query.to_string()),
            ("Filter", "*".to_string()),
            ("StartingIndex", start_str),
            ("RequestedCount", count_str),
            ("SortCriteria", String::new()),
        ];

        let response = self.invoke_content_directory("Search", Some("search"), args)?;
        let envelope = response.envelope.ok_or_else(|| {
            ControlPointError::MediaServerError(format!("Missing SOAP envelope in Search response"))
        })?;
        let didl_xml = extract_result_payload(&envelope, "SearchResponse")?;
        map_didl_entries(&didl_xml)
    }

    fn invoke_content_directory(
        &self,
        action: &str,
        op_name: Option<&str>,
        args: Vec<(&'static str, String)>,
    ) -> Result<SoapCallResult, ControlPointError> {
        let op = op_name.unwrap_or(action);
        let (control_url, service_type) = self.content_directory_endpoints(op)?;
        let borrowed_args: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();

        let call_result = invoke_upnp_action_with_timeout(
            control_url,
            service_type,
            action,
            &borrowed_args,
            Some(DEFAULT_HTTP_TIMEOUT),
        )?;

        if !call_result.status.is_success() {
            if let Some(env) = &call_result.envelope {
                if let Some(err) = parse_upnp_error(env) {
                    if should_map_to_not_supported(action, op_name, err.error_code) {
                        return Err(ControlPointError::upnp_operation_not_supported(
                            op,
                            "UpnpMediaServer",
                        ));
                    }

                    return Err(ControlPointError::MediaServerError(format!(
                        "{} failed with UPnP error {}: {}",
                        action, err.error_code, err.error_description
                    )));
                }
            }

            return Err(ControlPointError::MediaServerError(format!(
                "{} failed with HTTP status {} and body: {}",
                action, call_result.status, call_result.raw_body
            )));
        }

        if let Some(env) = &call_result.envelope {
            if let Some(err) = parse_upnp_error(env) {
                if should_map_to_not_supported(action, op_name, err.error_code) {
                    return Err(ControlPointError::upnp_operation_not_supported(
                        op,
                        "UpnpMediaServer",
                    ));
                }

                return Err(ControlPointError::MediaServerError(format!(
                    "{} returned UPnP error {}: {}",
                    action, err.error_code, err.error_description
                )));
            }
        }

        Ok(call_result)
    }

    fn content_directory_endpoints(
        &self,
        op_name: &str,
    ) -> Result<(&str, &str), ControlPointError> {
        if !self.has_content_directory {
            return Err(ControlPointError::upnp_operation_not_supported(
                op_name,
                "UpnpMediaServer",
            ));
        }

        let control_url = self
            .content_directory_control_url
            .as_deref()
            .ok_or_else(|| {
                ControlPointError::upnp_operation_not_supported(op_name, "UpnpMediaServer")
            })?;
        let service_type = self
            .content_directory_service_type
            .as_deref()
            .ok_or_else(|| {
                ControlPointError::upnp_operation_not_supported(op_name, "UpnpMediaServer")
            })?;

        Ok((control_url, service_type))
    }
}

impl DeviceOnline for UpnpMediaServer {
    fn is_online(&self) -> bool {
        self.connection
            .lock()
            .expect("Connection Mutex Poisoned")
            .is_online()
    }
    fn last_seen(&self) -> SystemTime {
        self.connection
            .lock()
            .expect("Connection Mutex Poisoned")
            .last_seen()
    }
    fn has_been_seen_now(&self, max_age: u32) {
        self.connection
            .lock()
            .expect("Connection Mutex Poisoned")
            .has_been_seen_now(max_age)
    }
    fn mark_as_offline(&self) {
        self.connection
            .lock()
            .expect("Connection Mutex Poisoned")
            .mark_as_offline()
    }
    fn max_age(&self) -> u32 {
        self.connection
            .lock()
            .expect("Connection Mutex Poisoned")
            .max_age()
    }
}

impl DeviceIdentity for UpnpMediaServer {
    fn id(&self) -> DeviceId {
        self.id.clone()
    }
    fn udn(&self) -> &str {
        &self.udn
    }
    fn friendly_name(&self) -> &str {
        &self.friendly_name
    }
    fn model_name(&self) -> &str {
        &self.model_name
    }
    fn manufacturer(&self) -> &str {
        &self.manufacturer
    }
    fn location(&self) -> &str {
        &self.location
    }
    fn server_header(&self) -> &str {
        &self.server_header
    }

    fn is_a_media_server(&self) -> bool {
        true
    }
}

/// Simplified view over a DIDL-Lite resource entry.
#[derive(Clone, Debug)]
pub struct MediaResource {
    pub uri: String,
    pub protocol_info: String,
    pub duration: Option<String>,
}

impl MediaResource {
    /// Returns true if this resource represents audio content.
    pub fn is_audio(&self) -> bool {
        let lower = self.protocol_info.to_ascii_lowercase();

        // Standard case: audio/* MIME types
        if lower.contains("audio/") {
            return true;
        }

        // List of known audio format subtypes (the part after the /)
        // These are recognized regardless of the MIME type prefix
        const AUDIO_FORMATS: &[&str] = &[
            "flac", "ogg", "opus", "vorbis", "mp3", "mpeg", "mp4", "m4a", "aac", "wav", "wave",
            "pcm", "wma", "webm", "ape", "alac", "aiff", "dsd", "dsf", "dff",
        ];

        // Check if any known audio format appears in the protocol_info
        for format in AUDIO_FORMATS {
            if lower.contains(format) {
                return true;
            }
        }

        // protocolInfo format: protocol:network:contentFormat:additionalInfo
        // Extract the MIME type (3rd field) for more precise checking
        if let Some(mime) = lower.split(':').nth(2) {
            // Check if it's audio/* or contains a known audio format
            if mime.starts_with("audio/") {
                return true;
            }

            // Check the subtype (part after /) for known audio formats
            if let Some(subtype) = mime.split('/').nth(1) {
                for format in AUDIO_FORMATS {
                    if subtype.contains(format) {
                        return true;
                    }
                }
            }
        }

        false
    }
}

/// Representation of either a container or an item returned by ContentDirectory.
#[derive(Clone, Debug)]
pub struct MediaEntry {
    pub id: String,
    pub parent_id: String,
    pub title: String,
    pub is_container: bool,
    pub class: String,
    pub resources: Vec<MediaResource>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub album_art_uri: Option<String>,
    pub date: Option<String>,
    pub track_number: Option<String>,
    pub creator: Option<String>,
}

/// Backend-agnostic media browsing contract.
pub trait MediaBrowser {
    fn browse_root(&self) -> Result<Vec<MediaEntry>, ControlPointError>;
    fn browse_children(
        &self,
        object_id: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<MediaEntry>, ControlPointError>;
    fn browse_object(&self, object_id: &str) -> Result<MediaEntry, ControlPointError>;
    fn search(
        &self,
        container_id: &str,
        query: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<MediaEntry>, ControlPointError>;
}

/// Façade over every supported media server backend.
#[derive(Clone, Debug)]
pub enum MusicServer {
    Upnp(UpnpMediaServer),
}

impl MusicServer {
    pub fn from_server_info(info: &UpnpMediaServer) -> Result<MusicServer, ControlPointError> {
        Ok(MusicServer::Upnp(info.clone()))
    }

    pub fn has_content_directory(&self) -> bool {
        match self {
            MusicServer::Upnp(u) => u.has_content_directory(),
        }
    }
}

impl DeviceOnline for MusicServer {
    fn is_online(&self) -> bool {
        match self {
            MusicServer::Upnp(u) => u.is_online(),
        }
    }
    fn last_seen(&self) -> SystemTime {
        match self {
            MusicServer::Upnp(u) => u.last_seen(),
        }
    }
    fn has_been_seen_now(&self, max_age: u32) {
        match self {
            MusicServer::Upnp(u) => u.has_been_seen_now(max_age),
        }
    }
    fn mark_as_offline(&self) {
        match self {
            MusicServer::Upnp(u) => u.mark_as_offline(),
        }
    }
    fn max_age(&self) -> u32 {
        match self {
            MusicServer::Upnp(u) => u.max_age(),
        }
    }
}

impl DeviceIdentity for MusicServer {
    fn id(&self) -> DeviceId {
        match self {
            MusicServer::Upnp(u) => u.id(),
        }
    }

    fn udn(&self) -> &str {
        match self {
            MusicServer::Upnp(u) => u.udn(),
        }
    }
    fn friendly_name(&self) -> &str {
        match self {
            MusicServer::Upnp(u) => u.friendly_name(),
        }
    }
    fn model_name(&self) -> &str {
        match self {
            MusicServer::Upnp(u) => u.model_name(),
        }
    }
    fn manufacturer(&self) -> &str {
        match self {
            MusicServer::Upnp(u) => u.manufacturer(),
        }
    }
    fn location(&self) -> &str {
        match self {
            MusicServer::Upnp(u) => u.location(),
        }
    }
    fn server_header(&self) -> &str {
        match self {
            MusicServer::Upnp(u) => u.server_header(),
        }
    }

    fn is_a_media_server(&self) -> bool {
        true
    }
}

impl MediaBrowser for MusicServer {
    fn browse_root(&self) -> Result<Vec<MediaEntry>, ControlPointError> {
        match self {
            MusicServer::Upnp(upnp) => upnp.browse_root(),
        }
    }

    fn browse_children(
        &self,
        object_id: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<MediaEntry>, ControlPointError> {
        match self {
            MusicServer::Upnp(upnp) => upnp.browse_children(object_id, start, count),
        }
    }

    fn browse_object(&self, object_id: &str) -> Result<MediaEntry, ControlPointError> {
        match self {
            MusicServer::Upnp(upnp) => upnp.browse_object(object_id),
        }
    }

    fn search(
        &self,
        container_id: &str,
        query: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<MediaEntry>, ControlPointError> {
        match self {
            MusicServer::Upnp(upnp) => upnp.search(container_id, query, start, count),
        }
    }
}

/// Helper to convert a MediaEntry to a PlaybackItem.
///
/// This function filters out containers and entries without audio resources,
/// returning None for items that cannot be played.
pub fn playback_item_from_entry(
    server: Arc<MusicServer>,
    entry: &MediaEntry,
) -> Option<PlaybackItem> {
    // Ignore containers
    if entry.is_container {
        debug!(
            server_id = server.id().0.as_str(),
            entry_id = entry.id.as_str(),
            title = entry.title.as_str(),
            class = entry.class.as_str(),
            "Skipping container entry"
        );
        return None;
    }

    // Skip "live stream" entries (heuristic from example)
    if entry.title.to_ascii_lowercase().contains("live stream") {
        debug!(
            server_id = server.id().0.as_str(),
            entry_id = entry.id.as_str(),
            title = entry.title.as_str(),
            "Skipping 'live stream' entry"
        );
        return None;
    }

    // Find an audio resource
    let resource = entry.resources.iter().find(|res| res.is_audio());

    if resource.is_none() {
        warn!(
            server_id = server.id().0.as_str(),
            entry_id = entry.id.as_str(),
            title = entry.title.as_str(),
            class = entry.class.as_str(),
            resource_count = entry.resources.len(),
            resources = ?entry.resources.iter().map(|r| &r.protocol_info).collect::<Vec<_>>(),
            "No audio resource found for entry"
        );
        return None;
    }

    let resource = resource.unwrap();

    let metadata = TrackMetadata {
        title: Some(entry.title.clone()),
        artist: entry.artist.clone(),
        album: entry.album.clone(),
        genre: entry.genre.clone(),
        album_art_uri: entry.album_art_uri.clone(),
        date: entry.date.clone(),
        track_number: entry.track_number.clone(),
        creator: entry.creator.clone(),
    };

    debug!(
        server_id = server.id().0.as_str(),
        entry_id = entry.id.as_str(),
        title = entry.title.as_str(),
        uri = resource.uri.as_str(),
        "Created playback item"
    );

    Some(PlaybackItem {
        media_server_id: server.id().clone(),
        backend_id: usize::MAX,
        didl_id: entry.id.clone(),
        uri: resource.uri.clone(),
        protocol_info: resource.protocol_info.clone(),
        metadata: Some(metadata),
    })
}

impl MediaBrowser for UpnpMediaServer {
    fn browse_root(&self) -> Result<Vec<MediaEntry>, ControlPointError> {
        self.browse_with_flag("0", "BrowseDirectChildren", 0, 0)
    }

    fn browse_children(
        &self,
        object_id: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<MediaEntry>, ControlPointError> {
        self.browse_with_flag(object_id, "BrowseDirectChildren", start, count)
    }

    fn browse_object(&self, object_id: &str) -> Result<MediaEntry, ControlPointError> {
        let entries = self.browse_with_flag(object_id, "BrowseMetadata", 0, 1)?;
        entries.into_iter().next().ok_or_else(|| {
            ControlPointError::MediaServerError(format!(
                "Object {} was not returned by the server",
                object_id
            ))
        })
    }

    fn search(
        &self,
        container_id: &str,
        query: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<MediaEntry>, ControlPointError> {
        self.search_impl(container_id, query, start, count)
    }
}

fn map_didl_entries(xml: &str) -> Result<Vec<MediaEntry>, ControlPointError> {
    let trimmed = xml.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let didl: DIDLLite = pmodidl::parse_metadata::<DIDLLite>(trimmed)
        .map_err(|err| {
            ControlPointError::MediaServerError(format!(
                "Failed to parse DIDL-Lite payload: {}",
                err
            ))
        })?
        .data;

    let mut entries = Vec::new();

    for container in didl.containers {
        entries.push(MediaEntry {
            id: container.id,
            parent_id: container.parent_id,
            title: container.title,
            is_container: true,
            class: container.class,
            resources: Vec::new(),
            artist: None,
            album: None,
            genre: None,
            album_art_uri: None,
            date: None,
            track_number: None,
            creator: None,
        });
    }

    for item in didl.items {
        let resources = item
            .resources
            .into_iter()
            .filter_map(|res| {
                if res.url.trim().is_empty() {
                    return None;
                }
                Some(MediaResource {
                    uri: res.url,
                    protocol_info: res.protocol_info,
                    duration: res.duration,
                })
            })
            .collect();

        entries.push(MediaEntry {
            id: item.id,
            parent_id: item.parent_id,
            title: item.title,
            is_container: false,
            class: item.class,
            resources,
            artist: item.artist,
            album: item.album,
            genre: item.genre,
            album_art_uri: item.album_art,
            date: item.date,
            track_number: item.original_track_number,
            creator: item.creator,
        });
    }

    Ok(entries)
}

fn extract_result_payload(
    envelope: &SoapEnvelope,
    response_suffix: &str,
) -> Result<String, ControlPointError> {
    let response =
        find_child_with_suffix(&envelope.body.content, response_suffix).ok_or_else(|| {
            ControlPointError::MediaServerError(format!(
                "Missing {} element in SOAP body",
                response_suffix
            ))
        })?;
    let result_elem = find_child_with_suffix(response, "Result").ok_or_else(|| {
        ControlPointError::MediaServerError(format!(
            "Missing Result element in {}",
            response_suffix
        ))
    })?;

    let payload = result_elem
        .get_text()
        .map(|t| t.to_string())
        .unwrap_or_default();

    Ok(payload)
}

fn find_child_with_suffix<'a>(parent: &'a Element, suffix: &str) -> Option<&'a Element> {
    parent.children.iter().find_map(|node| match node {
        XMLNode::Element(elem) if elem.name.ends_with(suffix) => Some(elem),
        _ => None,
    })
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

fn should_map_to_not_supported(action: &str, op_name: Option<&str>, error_code: u32) -> bool {
    if op_name.is_none() {
        return false;
    }

    let optional_code = error_codes::OPTIONAL_ACTION_NOT_IMPLEMENTED
        .parse::<u32>()
        .unwrap_or(602);
    let invalid_action_code = error_codes::INVALID_ACTION.parse::<u32>().unwrap_or(401);

    let cd_not_supported = matches!(action, "Search");

    cd_not_supported && (error_code == optional_code || error_code == invalid_action_code)
}

#[derive(Debug, Clone)]
struct UpnpError {
    pub error_code: u32,
    pub error_description: String,
}
