use anyhow::{anyhow, Result};

use crate::soap_client::{invoke_upnp_action, SoapCallResult};
use pmoupnp::soap::SoapEnvelope;
use xmltree::{Element, XMLNode};

#[derive(Debug, Clone)]
pub struct ConnectionManagerClient {
    pub control_url: String,
    pub service_type: String,
}

#[derive(Debug, Clone)]
pub struct ProtocolInfo {
    /// Liste brute des protocolInfo "source" (séparés par virgule dans UPnP)
    pub source: Vec<String>,
    /// Liste brute des protocolInfo "sink"
    pub sink: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub rcs_id: i32,
    pub av_transport_id: i32,
    pub protocol_info: String,
    pub peer_connection_manager: String,
    pub peer_connection_id: i32,
    pub direction: String,
    pub status: String,
}

impl ConnectionManagerClient {
    pub fn new(control_url: String, service_type: String) -> Self {
        Self {
            control_url,
            service_type,
        }
    }

    /// GetProtocolInfo
    pub fn get_protocol_info(&self) -> Result<ProtocolInfo> {
        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "GetProtocolInfo",
            &[],
        )?;

        ensure_success("GetProtocolInfo", &call_result)?;

        let envelope = call_result
            .envelope
            .as_ref()
            .ok_or_else(|| anyhow!("Missing SOAP envelope in GetProtocolInfo response"))?;

        if let Some(err) = parse_upnp_error(envelope) {
            return Err(anyhow!(
                "GetProtocolInfo returned UPnP error {}: {} (HTTP status {})",
                err.error_code,
                err.error_description,
                call_result.status
            ));
        }

        let response = find_child_with_suffix(&envelope.body.content, "GetProtocolInfoResponse")
            .ok_or_else(|| anyhow!("Missing GetProtocolInfoResponse element in SOAP body"))?;

        let source_text = extract_child_text_allow_empty(response, "Source")?;
        let sink_text = extract_child_text_allow_empty(response, "Sink")?;

        Ok(ProtocolInfo {
            source: split_list(&source_text),
            sink: split_list(&sink_text),
        })
    }

    /// GetCurrentConnectionIDs
    pub fn get_current_connection_ids(&self) -> Result<Vec<i32>> {
        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "GetCurrentConnectionIDs",
            &[],
        )?;

        ensure_success("GetCurrentConnectionIDs", &call_result)?;

        let envelope = call_result
            .envelope
            .as_ref()
            .ok_or_else(|| anyhow!("Missing SOAP envelope in GetCurrentConnectionIDs response"))?;

        if let Some(err) = parse_upnp_error(envelope) {
            return Err(anyhow!(
                "GetCurrentConnectionIDs returned UPnP error {}: {} (HTTP status {})",
                err.error_code,
                err.error_description,
                call_result.status
            ));
        }

        let response =
            find_child_with_suffix(&envelope.body.content, "GetCurrentConnectionIDsResponse")
                .ok_or_else(|| {
                    anyhow!("Missing GetCurrentConnectionIDsResponse element in SOAP body")
                })?;

        let ids_text = extract_child_text_allow_empty(response, "ConnectionIDs")?;
        let trimmed = ids_text.trim();

        if trimmed.is_empty() || trimmed == "0" {
            return Ok(Vec::new());
        }

        let mut ids = Vec::new();
        for part in trimmed.split(',') {
            let value = part.trim();
            if value.is_empty() {
                continue;
            }
            let parsed = value
                .parse::<i32>()
                .map_err(|_| anyhow!("Invalid ConnectionID value: {}", value))?;
            ids.push(parsed);
        }

        Ok(ids)
    }

    /// GetCurrentConnectionInfo
    pub fn get_current_connection_info(&self, connection_id: i32) -> Result<ConnectionInfo> {
        let connection_id_str = connection_id.to_string();
        let args = [("ConnectionID", connection_id_str.as_str())];

        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "GetCurrentConnectionInfo",
            &args,
        )?;

        ensure_success("GetCurrentConnectionInfo", &call_result)?;

        let envelope = call_result
            .envelope
            .as_ref()
            .ok_or_else(|| anyhow!("Missing SOAP envelope in GetCurrentConnectionInfo response"))?;

        if let Some(err) = parse_upnp_error(envelope) {
            return Err(anyhow!(
                "GetCurrentConnectionInfo returned UPnP error {}: {} (HTTP status {})",
                err.error_code,
                err.error_description,
                call_result.status
            ));
        }

        let response =
            find_child_with_suffix(&envelope.body.content, "GetCurrentConnectionInfoResponse")
                .ok_or_else(|| {
                    anyhow!("Missing GetCurrentConnectionInfoResponse element in SOAP body")
                })?;

        let rcs_id = extract_child_text(response, "RcsID")?
            .parse::<i32>()
            .map_err(|_| anyhow!("Invalid RcsID value in response"))?;

        let av_transport_id = extract_child_text(response, "AVTransportID")?
            .parse::<i32>()
            .map_err(|_| anyhow!("Invalid AVTransportID value in response"))?;

        let protocol_info = extract_child_text_allow_empty(response, "ProtocolInfo")?;
        let peer_connection_manager =
            extract_child_text_allow_empty(response, "PeerConnectionManager")?;

        let peer_connection_id = extract_child_text(response, "PeerConnectionID")?
            .parse::<i32>()
            .map_err(|_| anyhow!("Invalid PeerConnectionID value in response"))?;

        let direction = extract_child_text(response, "Direction")?;
        let status = extract_child_text(response, "Status")?;

        Ok(ConnectionInfo {
            rcs_id,
            av_transport_id,
            protocol_info,
            peer_connection_manager,
            peer_connection_id,
            direction,
            status,
        })
    }
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect()
}

fn ensure_success(action: &str, call_result: &SoapCallResult) -> Result<()> {
    if call_result.status.is_success() {
        return Ok(());
    }

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

    Err(anyhow!(
        "{action} failed with HTTP status {} and body: {}",
        call_result.status,
        call_result.raw_body
    ))
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

    let binding = error_code_elem.get_text()?;
    let error_code_text = binding.trim();
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
        .unwrap_or_else(|| String::from(""));

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
    let text = extract_child_text_allow_empty(parent, suffix)?;
    if text.is_empty() {
        return Err(anyhow!("{suffix} element missing text in response"));
    }
    Ok(text)
}

fn extract_child_text_allow_empty(parent: &Element, suffix: &str) -> Result<String> {
    let child = find_child_with_suffix(parent, suffix)
        .ok_or_else(|| anyhow!("Missing {suffix} element in response"))?;

    let text = child
        .get_text()
        .map(|t| t.trim().to_string())
        .unwrap_or_default();

    Ok(text)
}
