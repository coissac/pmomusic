use anyhow::{Result, anyhow};

use crate::{errors::ControlPointError, soap_client::{
    SoapCallResult, ensure_success, extract_child_text, extract_child_text_allow_empty, find_child_with_suffix, invoke_upnp_action, parse_upnp_error
}};

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
    pub fn get_protocol_info(&self) -> Result<ProtocolInfo, ControlPointError> {
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
            .ok_or_else(|| ControlPointError::UpnpError(format!("Missing SOAP envelope in GetProtocolInfo response")))?;

        if let Some(err) = parse_upnp_error(envelope) {
            return Err(ControlPointError::UpnpError(format!(
                "GetProtocolInfo returned UPnP error {}: {} (HTTP status {})",
                err.error_code,
                err.error_description,
                call_result.status
            )));
        }

        let response = find_child_with_suffix(&envelope.body.content, "GetProtocolInfoResponse")
            .ok_or_else(|| ControlPointError::UpnpError(format!("Missing GetProtocolInfoResponse element in SOAP body")))?;

        let source_text = extract_child_text_allow_empty(response, "Source")?;
        let sink_text = extract_child_text_allow_empty(response, "Sink")?;

        Ok(ProtocolInfo {
            source: split_list(&source_text),
            sink: split_list(&sink_text),
        })
    }

    /// GetCurrentConnectionIDs
    pub fn get_current_connection_ids(&self) -> Result<Vec<i32>, ControlPointError> {
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
            .ok_or_else(|| ControlPointError::UpnpError(format!("Missing SOAP envelope in GetCurrentConnectionIDs response")))?;

        if let Some(err) = parse_upnp_error(envelope) {
            return Err(ControlPointError::UpnpError(format!(
                "GetCurrentConnectionIDs returned UPnP error {}: {} (HTTP status {})",
                err.error_code,
                err.error_description,
                call_result.status
            )));
        }

        let response =
            find_child_with_suffix(&envelope.body.content, "GetCurrentConnectionIDsResponse")
                .ok_or_else(|| {
                    ControlPointError::UpnpError(format!("Missing GetCurrentConnectionIDsResponse element in SOAP body"))
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
                .map_err(|_| ControlPointError::UpnpError(format!("Invalid ConnectionID value: {}", value)))?;
            ids.push(parsed);
        }

        Ok(ids)
    }

    /// GetCurrentConnectionInfo
    pub fn get_current_connection_info(&self, connection_id: i32) -> Result<ConnectionInfo, ControlPointError> {
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
            .ok_or_else(|| ControlPointError::UpnpError(format!("Missing SOAP envelope in GetCurrentConnectionInfo response")))?;

        if let Some(err) = parse_upnp_error(envelope) {
            return Err(ControlPointError::UpnpError(format!(
                "GetCurrentConnectionInfo returned UPnP error {}: {} (HTTP status {})",
                err.error_code,
                err.error_description,
                call_result.status
            )));
        }

        let response =
            find_child_with_suffix(&envelope.body.content, "GetCurrentConnectionInfoResponse")
                .ok_or_else(|| {
                    ControlPointError::UpnpError(format!("Missing GetCurrentConnectionInfoResponse element in SOAP body"))
                })?;

        let rcs_id = extract_child_text(response, "RcsID")?
            .parse::<i32>()
            .map_err(|_| ControlPointError::UpnpError(format!("Invalid RcsID value in response")))?;

        let av_transport_id = extract_child_text(response, "AVTransportID")?
            .parse::<i32>()
            .map_err(|_| ControlPointError::UpnpError(format!("Invalid AVTransportID value in response")))?;

        let protocol_info = extract_child_text_allow_empty(response, "ProtocolInfo")?;
        let peer_connection_manager =
            extract_child_text_allow_empty(response, "PeerConnectionManager")?;

        let peer_connection_id = extract_child_text(response, "PeerConnectionID")?
            .parse::<i32>()
            .map_err(|_| ControlPointError::UpnpError(format!("Invalid PeerConnectionID value in response")))?;

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
