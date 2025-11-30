// pmocontrol/src/avtransport_client.rs

use anyhow::{anyhow, Result};
use crate::soap_client::{invoke_upnp_action, SoapCallResult};
use pmoupnp::soap::SoapEnvelope;
use xmltree::{Element, XMLNode};

#[derive(Debug, Clone)]
pub struct AvTransportClient {
    pub control_url: String,
    pub service_type: String,
}

#[derive(Debug, Clone)]
pub struct TransportInfo {
    pub current_transport_state: String,
    pub current_transport_status: String,
    pub current_speed: String,
}

impl AvTransportClient {
    pub fn new(control_url: String, service_type: String) -> Self {
        Self {
            control_url,
            service_type,
        }
    }

    /// AVTransport:1 — GetTransportInfo
    pub fn get_transport_info(&self, instance_id: u32) -> Result<TransportInfo> {
        let instance_id_str = instance_id.to_string();
        let args = [("InstanceID", instance_id_str.as_str())];

        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "GetTransportInfo",
            &args,
        )?;

        if !call_result.status.is_success() {
            return Err(anyhow!(
                "GetTransportInfo failed with HTTP status {}",
                call_result.status
            ));
        }

        let envelope = call_result
            .envelope
            .as_ref()
            .ok_or_else(|| anyhow!("Missing SOAP envelope in GetTransportInfo response"))?;

        parse_transport_info(envelope)
    }

    /// AVTransport:1 — SetAVTransportURI
    ///
    /// Pour l’instant on force `InstanceID = 0`, ce qui couvre la majorité
    /// des MediaRenderers UPnP AV (un seul instance de transport).
    ///
    /// - `uri`  : CurrentURI
    /// - `meta` : CurrentURIMetaData (DIDL-Lite ou chaîne vide)
    pub fn set_av_transport_uri(&self, uri: &str, meta: &str) -> Result<()> {
        let args = [
            ("InstanceID", "0"),
            ("CurrentURI", uri),
            ("CurrentURIMetaData", meta),
        ];

        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "SetAVTransportURI",
            &args,
        )?;

        handle_action_response("SetAVTransportURI", &call_result)
    }

    /// AVTransport:1 — Play
    pub fn play(&self, instance_id: u32, speed: &str) -> Result<()> {
        let instance_id_str = instance_id.to_string();
        let args = [
            ("InstanceID", instance_id_str.as_str()),
            ("Speed", speed),
        ];

        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "Play",
            &args,
        )?;

        handle_action_response("Play", &call_result)
    }

    /// AVTransport:1 — Pause
    pub fn pause(&self, instance_id: u32) -> Result<()> {
        let instance_id_str = instance_id.to_string();
        let args = [("InstanceID", instance_id_str.as_str())];

        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "Pause",
            &args,
        )?;

        handle_action_response("Pause", &call_result)
    }

    /// AVTransport:1 — Stop
    pub fn stop(&self, instance_id: u32) -> Result<()> {
        let instance_id_str = instance_id.to_string();
        let args = [("InstanceID", instance_id_str.as_str())];

        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "Stop",
            &args,
        )?;

        handle_action_response("Stop", &call_result)
    }

    /// AVTransport:1 — Seek
    pub fn seek(&self, instance_id: u32, unit: &str, target: &str) -> Result<()> {
        let instance_id_str = instance_id.to_string();
        let args = [
            ("InstanceID", instance_id_str.as_str()),
            ("Unit", unit),
            ("Target", target),
        ];

        let call_result = invoke_upnp_action(
            &self.control_url,
            &self.service_type,
            "Seek",
            &args,
        )?;

        handle_action_response("Seek", &call_result)
    }
}

fn handle_action_response(action: &str, call_result: &SoapCallResult) -> Result<()> {
    if !call_result.status.is_success() {
        if let Some(env) = &call_result.envelope {
            if let Some(upnp_error) = parse_upnp_error(env) {
                return Err(anyhow!(
                    "{action} failed with UPnP error {}: {} (HTTP status {})",
                    upnp_error.error_code,
                    upnp_error.error_description,
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

    if let Some(env) = &call_result.envelope {
        if let Some(upnp_error) = parse_upnp_error(env) {
            return Err(anyhow!(
                "{action} returned UPnP error {}: {} (HTTP status {})",
                upnp_error.error_code,
                upnp_error.error_description,
                call_result.status
            ));
        }
    }

    Ok(())
}

fn parse_transport_info(envelope: &SoapEnvelope) -> Result<TransportInfo> {
    let response = find_child_with_suffix(&envelope.body.content, "GetTransportInfoResponse")
        .ok_or_else(|| anyhow!("Missing GetTransportInfoResponse element in SOAP body"))?;

    let current_transport_state =
        extract_child_text(response, "CurrentTransportState")?;
    let current_transport_status =
        extract_child_text(response, "CurrentTransportStatus")?;
    let current_speed = extract_child_text(response, "CurrentSpeed")?;

    Ok(TransportInfo {
        current_transport_state,
        current_transport_status,
        current_speed,
    })
}

/// Représente une erreur UPnP extraite d’un SOAP Fault.
#[derive(Debug, Clone)]
struct UpnpError {
    pub error_code: u32,
    pub error_description: String,
}

/// Parse un éventuel SOAP Fault contenant un UPnPError.
///
/// Schéma typique (SOAP 1.1) :
///
/// ```xml
/// <s:Body>
///   <s:Fault>
///     <faultcode>...</faultcode>
///     <faultstring>...</faultstring>
///     <detail>
///       <UPnPError xmlns="urn:schemas-upnp-org:control-1-0">
///         <errorCode>401</errorCode>
///         <errorDescription>Invalid Action</errorDescription>
///       </UPnPError>
///     </detail>
///   </s:Fault>
/// </s:Body>
/// ```
fn parse_upnp_error(envelope: &SoapEnvelope) -> Option<UpnpError> {
    let fault = find_child_with_suffix(&envelope.body.content, "Fault")?;
    let detail = find_child_with_suffix(fault, "detail")?;
    let upnp_error = find_child_with_suffix(detail, "UPnPError")?;

    // errorCode (obligatoire dans la spec)
    let error_code_elem = upnp_error.children.iter().find_map(|node| match node {
        XMLNode::Element(elem) if elem.name.ends_with("errorCode") => Some(elem),
        _ => None,
    })?;

    let binding = error_code_elem.get_text()?;
    let error_code_text = binding.trim();
    let error_code = error_code_text.parse::<u32>().ok()?;

    // errorDescription (optionnel, mais utile)
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
        .ok_or_else(|| anyhow!("Missing {suffix} element in GetTransportInfoResponse"))?;

    let text = child
        .get_text()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| anyhow!("{suffix} element missing text in GetTransportInfoResponse"))?;

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pmoupnp::soap::{SoapBody, SoapEnvelope};

    fn text_element(name: &str, text: &str) -> Element {
        let mut elem = Element::new(name);
        elem.children.push(XMLNode::Text(text.to_string()));
        elem
    }

    #[test]
    fn parse_transport_info_extracts_fields() {
        let mut response = Element::new("u:GetTransportInfoResponse");
        response.children.push(XMLNode::Element(text_element(
            "CurrentTransportState",
            "STOPPED",
        )));
        response.children.push(XMLNode::Element(text_element(
            "CurrentTransportStatus",
            "OK",
        )));
        response
            .children
            .push(XMLNode::Element(text_element("CurrentSpeed", "1")));

        let mut body = Element::new("s:Body");
        body.children.push(XMLNode::Element(response));

        let envelope = SoapEnvelope {
            header: None,
            body: SoapBody { content: body },
        };

        let info = parse_transport_info(&envelope).unwrap();
        assert_eq!(info.current_transport_state, "STOPPED");
        assert_eq!(info.current_transport_status, "OK");
        assert_eq!(info.current_speed, "1");
    }
}

#[cfg(test)]
mod upnp_error_tests {
    use super::*;
    use pmoupnp::soap::{SoapBody, SoapEnvelope};

    fn text_element(name: &str, text: &str) -> Element {
        let mut elem = Element::new(name);
        elem.children.push(XMLNode::Text(text.to_string()));
        elem
    }

    #[test]
    fn parse_upnp_error_extracts_error_code_and_description() {
        let error_code = text_element("errorCode", "401");
        let error_description = text_element("errorDescription", "Invalid Action");

        let mut upnp_error = Element::new("UPnPError");
        upnp_error.children.push(XMLNode::Element(error_code));
        upnp_error
            .children
            .push(XMLNode::Element(error_description));

        let mut detail = Element::new("detail");
        detail.children.push(XMLNode::Element(upnp_error));

        let mut fault = Element::new("s:Fault");
        fault.children.push(XMLNode::Element(detail));

        let mut body = Element::new("s:Body");
        body.children.push(XMLNode::Element(fault));

        let envelope = SoapEnvelope {
            header: None,
            body: SoapBody { content: body },
        };

        let err = parse_upnp_error(&envelope).expect("Expected UPnPError");
        assert_eq!(err.error_code, 401);
        assert_eq!(err.error_description, "Invalid Action");
    }
}
