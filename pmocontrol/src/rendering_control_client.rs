use anyhow::{anyhow, Result};
use crate::soap_client::{invoke_upnp_action, SoapCallResult};
use pmoupnp::soap::SoapEnvelope;
use xmltree::{Element, XMLNode};

#[derive(Debug, Clone)]
pub struct RenderingControlClient {
    pub control_url: String,
    pub service_type: String,
}

impl RenderingControlClient {
    pub fn new(control_url: String, service_type: String) -> Self {
        Self {
            control_url,
            service_type,
        }
    }

    /// RenderingControl:1 — GetVolume
    pub fn get_volume(&self, instance_id: u32, channel: &str) -> Result<u16> {
        let instance_id_str = instance_id.to_string();
        let args = [
            ("InstanceID", instance_id_str.as_str()),
            ("Channel", channel),
        ];

        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "GetVolume",
            &args,
        )?;

        ensure_success("GetVolume", &call_result)?;

        let envelope = call_result
            .envelope
            .as_ref()
            .ok_or_else(|| anyhow!("Missing SOAP envelope in GetVolume response"))?;

        if let Some(err) = parse_upnp_error(envelope) {
            return Err(anyhow!(
                "GetVolume returned UPnP error {}: {} (HTTP status {})",
                err.error_code,
                err.error_description,
                call_result.status
            ));
        }

        let response =
            find_child_with_suffix(&envelope.body.content, "GetVolumeResponse")
                .ok_or_else(|| anyhow!("Missing GetVolumeResponse element in SOAP body"))?;

        let text = extract_child_text(response, "CurrentVolume")?;
        let volume = text
            .parse::<u16>()
            .map_err(|_| anyhow!("Invalid CurrentVolume value: {}", text))?;

        Ok(volume)
    }

    /// RenderingControl:1 — SetVolume
    pub fn set_volume(&self, instance_id: u32, channel: &str, volume: u16) -> Result<()> {
        let instance_id_str = instance_id.to_string();
        let volume_str = volume.to_string();
        let args = [
            ("InstanceID", instance_id_str.as_str()),
            ("Channel", channel),
            ("DesiredVolume", volume_str.as_str()),
        ];

        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "SetVolume",
            &args,
        )?;

        handle_action_response("SetVolume", &call_result)
    }

    /// RenderingControl:1 — GetMute
    pub fn get_mute(&self, instance_id: u32, channel: &str) -> Result<bool> {
        let instance_id_str = instance_id.to_string();
        let args = [
            ("InstanceID", instance_id_str.as_str()),
            ("Channel", channel),
        ];

        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "GetMute",
            &args,
        )?;

        ensure_success("GetMute", &call_result)?;

        let envelope = call_result
            .envelope
            .as_ref()
            .ok_or_else(|| anyhow!("Missing SOAP envelope in GetMute response"))?;

        if let Some(err) = parse_upnp_error(envelope) {
            return Err(anyhow!(
                "GetMute returned UPnP error {}: {} (HTTP status {})",
                err.error_code,
                err.error_description,
                call_result.status
            ));
        }

        let response = find_child_with_suffix(&envelope.body.content, "GetMuteResponse")
            .ok_or_else(|| anyhow!("Missing GetMuteResponse element in SOAP body"))?;

        let text = extract_child_text(response, "CurrentMute")?;
        let mute = match text.as_str() {
            "0" => false,
            "1" => true,
            _ => {
                return Err(anyhow!(
                    "Invalid CurrentMute value: {} (expected 0 or 1)",
                    text
                ))
            }
        };

        Ok(mute)
    }

    /// RenderingControl:1 — SetMute
    pub fn set_mute(&self, instance_id: u32, channel: &str, mute: bool) -> Result<()> {
        let instance_id_str = instance_id.to_string();
        let mute_str = if mute { "1" } else { "0" };
        let args = [
            ("InstanceID", instance_id_str.as_str()),
            ("Channel", channel),
            ("DesiredMute", mute_str),
        ];

        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "SetMute",
            &args,
        )?;

        handle_action_response("SetMute", &call_result)
    }
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

fn handle_action_response(action: &str, call_result: &SoapCallResult) -> Result<()> {
    ensure_success(action, call_result)?;

    if let Some(env) = &call_result.envelope {
        if let Some(err) = parse_upnp_error(env) {
            return Err(anyhow!(
                "{action} returned UPnP error {}: {} (HTTP status {})",
                err.error_code,
                err.error_description,
                call_result.status
            ));
        }
    }

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
    let child = find_child_with_suffix(parent, suffix)
        .ok_or_else(|| anyhow!("Missing {suffix} element in response"))?;

    let text = child
        .get_text()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| anyhow!("{suffix} element missing text in response"))?;

    Ok(text)
}
