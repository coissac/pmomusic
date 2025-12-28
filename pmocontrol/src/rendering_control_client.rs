use crate::{
    errors::ControlPointError,
    soap_client::{
        SoapCallResult, ensure_success, extract_child_text, find_child_with_suffix,
        handle_action_response, invoke_upnp_action, parse_upnp_error,
    },
};
use anyhow::{Result, anyhow};
use tracing::debug;

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
    pub fn get_volume(&self, instance_id: u32, channel: &str) -> Result<u16, ControlPointError> {
        debug!("GetVolume: {}", instance_id);
        let instance_id_str = instance_id.to_string();
        let args = [
            ("InstanceID", instance_id_str.as_str()),
            ("Channel", channel),
        ];

        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "GetVolume", &args)?;

        ensure_success("GetVolume", &call_result)?;

        let envelope = call_result.envelope.as_ref().ok_or_else(|| {
            ControlPointError::UpnpError(format!("Missing SOAP envelope in GetVolume response"))
        })?;

        if let Some(err) = parse_upnp_error(envelope) {
            return Err(ControlPointError::UpnpError(format!(
                "GetVolume returned UPnP error {}: {} (HTTP status {})",
                err.error_code, err.error_description, call_result.status
            )));
        }

        let response = find_child_with_suffix(&envelope.body.content, "GetVolumeResponse")
            .ok_or_else(|| {
                ControlPointError::UpnpError(format!(
                    "Missing GetVolumeResponse element in SOAP body"
                ))
            })?;

        let text = extract_child_text(response, "CurrentVolume")?;
        let volume = text.parse::<u16>().map_err(|_| {
            ControlPointError::UpnpError(format!("Invalid CurrentVolume value: {}", text))
        })?;

        Ok(volume)
    }

    /// RenderingControl:1 — SetVolume
    pub fn set_volume(
        &self,
        instance_id: u32,
        channel: &str,
        volume: u16,
    ) -> Result<(), ControlPointError> {
        let instance_id_str = instance_id.to_string();
        let volume_str = volume.to_string();
        let args = [
            ("InstanceID", instance_id_str.as_str()),
            ("Channel", channel),
            ("DesiredVolume", volume_str.as_str()),
        ];

        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "SetVolume", &args)?;

        handle_action_response("SetVolume", &call_result)
    }

    /// RenderingControl:1 — GetMute
    pub fn get_mute(&self, instance_id: u32, channel: &str) -> Result<bool, ControlPointError> {
        let instance_id_str = instance_id.to_string();
        let args = [
            ("InstanceID", instance_id_str.as_str()),
            ("Channel", channel),
        ];

        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "GetMute", &args)?;

        ensure_success("GetMute", &call_result)?;

        let envelope = call_result
            .envelope
            .as_ref()
            .ok_or_else(|| ControlPointError::UpnpError(format!("Missing SOAP envelope in GetMute response")))?;

        if let Some(err) = parse_upnp_error(envelope) {
            return Err(ControlPointError::UpnpError(format!(
                "GetMute returned UPnP error {}: {} (HTTP status {})",
                err.error_code,
                err.error_description,
                call_result.status
            )));
        }

        let response = find_child_with_suffix(&envelope.body.content, "GetMuteResponse")
            .ok_or_else(|| ControlPointError::UpnpError(format!("Missing GetMuteResponse element in SOAP body")))?;

        let text = extract_child_text(response, "CurrentMute")?;
        let mute = match text.as_str() {
            "0" => false,
            "1" => true,
            _ => {
                return Err(ControlPointError::UpnpError(format!(
                    "Invalid CurrentMute value: {} (expected 0 or 1)",
                    text
                )));
            }
        };

        Ok(mute)
    }

    /// RenderingControl:1 — SetMute
    pub fn set_mute(
        &self,
        instance_id: u32,
        channel: &str,
        mute: bool,
    ) -> Result<(), ControlPointError> {
        let instance_id_str = instance_id.to_string();
        let mute_str = if mute { "1" } else { "0" };
        let args = [
            ("InstanceID", instance_id_str.as_str()),
            ("Channel", channel),
            ("DesiredMute", mute_str),
        ];

        let call_result =
            invoke_upnp_action(&self.control_url, &self.service_type, "SetMute", &args)?;

        handle_action_response("SetMute", &call_result)
    }
}
