use std::time::{Duration, SystemTime};

use anyhow::{Result, anyhow};
use pmodidl::{self, DIDLLite};
use pmoupnp::soap::SoapEnvelope;
use pmoupnp::soap::error_codes;
use xmltree::{Element, XMLNode};

use crate::soap_client::{SoapCallResult, invoke_upnp_action_with_timeout};

/// Unique identifier for a media server registered by the control point.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ServerId(pub String);

/// Snapshot of a media server discovered through UPnP SSDP.
#[derive(Clone, Debug)]
pub struct MediaServerInfo {
    pub id: ServerId,
    pub udn: String,
    pub friendly_name: String,
    pub model_name: String,
    pub manufacturer: String,
    pub location: String,
    pub server_header: String,
    pub online: bool,
    pub last_seen: SystemTime,
    pub max_age: u32,
    pub has_content_directory: bool,
    pub content_directory_service_type: Option<String>,
    pub content_directory_control_url: Option<String>,
}

/// Simplified view over a DIDL-Lite resource entry.
#[derive(Clone, Debug)]
pub struct MediaResource {
    pub uri: String,
    pub protocol_info: String,
    pub duration: Option<String>,
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
    fn browse_root(&self) -> Result<Vec<MediaEntry>>;
    fn browse_children(&self, object_id: &str, start: u32, count: u32) -> Result<Vec<MediaEntry>>;
    fn browse_object(&self, object_id: &str) -> Result<MediaEntry>;
    fn search(
        &self,
        container_id: &str,
        query: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<MediaEntry>>;
}

/// Façade over every supported media server backend.
#[derive(Clone, Debug)]
pub enum MusicServer {
    Upnp(UpnpMediaServer),
}

impl MusicServer {
    pub fn from_info(info: &MediaServerInfo, timeout: Duration) -> Result<Self> {
        Ok(MusicServer::Upnp(UpnpMediaServer::new(
            info.clone(),
            timeout,
        )))
    }

    pub fn id(&self) -> &ServerId {
        match self {
            MusicServer::Upnp(upnp) => upnp.id(),
        }
    }

    pub fn info(&self) -> &MediaServerInfo {
        match self {
            MusicServer::Upnp(upnp) => upnp.info(),
        }
    }
}

impl MediaBrowser for MusicServer {
    fn browse_root(&self) -> Result<Vec<MediaEntry>> {
        match self {
            MusicServer::Upnp(upnp) => upnp.browse_root(),
        }
    }

    fn browse_children(&self, object_id: &str, start: u32, count: u32) -> Result<Vec<MediaEntry>> {
        match self {
            MusicServer::Upnp(upnp) => upnp.browse_children(object_id, start, count),
        }
    }

    fn browse_object(&self, object_id: &str) -> Result<MediaEntry> {
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
    ) -> Result<Vec<MediaEntry>> {
        match self {
            MusicServer::Upnp(upnp) => upnp.search(container_id, query, start, count),
        }
    }
}

/// Single UPnP ContentDirectory backend implementation.
#[derive(Clone, Debug)]
pub struct UpnpMediaServer {
    info: MediaServerInfo,
    timeout: Duration,
}

impl UpnpMediaServer {
    pub fn new(info: MediaServerInfo, timeout: Duration) -> Self {
        Self { info, timeout }
    }

    pub fn id(&self) -> &ServerId {
        &self.info.id
    }

    pub fn info(&self) -> &MediaServerInfo {
        &self.info
    }

    fn browse_with_flag(
        &self,
        object_id: &str,
        browse_flag: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<MediaEntry>> {
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
        let envelope = response
            .envelope
            .ok_or_else(|| anyhow!("Missing SOAP envelope in Browse response"))?;
        let didl_xml = extract_result_payload(&envelope, "BrowseResponse")?;
        map_didl_entries(&didl_xml)
    }

    fn search_impl(
        &self,
        container_id: &str,
        query: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<MediaEntry>> {
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
        let envelope = response
            .envelope
            .ok_or_else(|| anyhow!("Missing SOAP envelope in Search response"))?;
        let didl_xml = extract_result_payload(&envelope, "SearchResponse")?;
        map_didl_entries(&didl_xml)
    }

    fn invoke_content_directory(
        &self,
        action: &str,
        op_name: Option<&str>,
        args: Vec<(&'static str, String)>,
    ) -> Result<SoapCallResult> {
        let op = op_name.unwrap_or(action);
        let (control_url, service_type) = self.content_directory_endpoints(op)?;
        let borrowed_args: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();

        let call_result = invoke_upnp_action_with_timeout(
            control_url,
            service_type,
            action,
            &borrowed_args,
            Some(self.timeout),
        )?;

        if !call_result.status.is_success() {
            if let Some(env) = &call_result.envelope {
                if let Some(err) = parse_upnp_error(env) {
                    if should_map_to_not_supported(action, op_name, err.error_code) {
                        return Err(server_op_not_supported(op, "UpnpMediaServer"));
                    }

                    return Err(anyhow!(
                        "{} failed with UPnP error {}: {}",
                        action,
                        err.error_code,
                        err.error_description
                    ));
                }
            }

            return Err(anyhow!(
                "{} failed with HTTP status {} and body: {}",
                action,
                call_result.status,
                call_result.raw_body
            ));
        }

        if let Some(env) = &call_result.envelope {
            if let Some(err) = parse_upnp_error(env) {
                if should_map_to_not_supported(action, op_name, err.error_code) {
                    return Err(server_op_not_supported(op, "UpnpMediaServer"));
                }

                return Err(anyhow!(
                    "{} returned UPnP error {}: {}",
                    action,
                    err.error_code,
                    err.error_description
                ));
            }
        }

        Ok(call_result)
    }

    fn content_directory_endpoints(&self, op_name: &str) -> Result<(&str, &str)> {
        if !self.info.has_content_directory {
            return Err(server_op_not_supported(op_name, "UpnpMediaServer"));
        }

        let control_url = self
            .info
            .content_directory_control_url
            .as_deref()
            .ok_or_else(|| server_op_not_supported(op_name, "UpnpMediaServer"))?;
        let service_type = self
            .info
            .content_directory_service_type
            .as_deref()
            .ok_or_else(|| server_op_not_supported(op_name, "UpnpMediaServer"))?;

        Ok((control_url, service_type))
    }
}

impl MediaBrowser for UpnpMediaServer {
    fn browse_root(&self) -> Result<Vec<MediaEntry>> {
        self.browse_with_flag("0", "BrowseDirectChildren", 0, 0)
    }

    fn browse_children(&self, object_id: &str, start: u32, count: u32) -> Result<Vec<MediaEntry>> {
        self.browse_with_flag(object_id, "BrowseDirectChildren", start, count)
    }

    fn browse_object(&self, object_id: &str) -> Result<MediaEntry> {
        let entries = self.browse_with_flag(object_id, "BrowseMetadata", 0, 1)?;
        entries
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Object {} was not returned by the server", object_id))
    }

    fn search(
        &self,
        container_id: &str,
        query: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<MediaEntry>> {
        self.search_impl(container_id, query, start, count)
    }
}

fn map_didl_entries(xml: &str) -> Result<Vec<MediaEntry>> {
    let trimmed = xml.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let didl: DIDLLite = pmodidl::parse_metadata::<DIDLLite>(trimmed)
        .map_err(|err| anyhow!("Failed to parse DIDL-Lite payload: {}", err))?
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

fn extract_result_payload(envelope: &SoapEnvelope, response_suffix: &str) -> Result<String> {
    let response = find_child_with_suffix(&envelope.body.content, response_suffix)
        .ok_or_else(|| anyhow!("Missing {} element in SOAP body", response_suffix))?;
    let result_elem = find_child_with_suffix(response, "Result")
        .ok_or_else(|| anyhow!("Missing Result element in {}", response_suffix))?;

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

fn server_op_not_supported(op: &str, backend: &str) -> anyhow::Error {
    anyhow!(
        "MusicServer operation '{}' is not supported by backend '{}'",
        op,
        backend
    )
}

#[derive(Debug, Clone)]
struct UpnpError {
    pub error_code: u32,
    pub error_description: String,
}
