use std::time::Duration;

use anyhow::{Context, Result};
use pmoupnp::soap::{build_soap_request, parse_soap_envelope, SoapEnvelope};
use tracing::{debug, trace, warn};
use ureq::Agent;

/// Result of a SOAP call:
/// - HTTP status code
/// - raw XML body (always)
/// - parsed SOAP envelope if parsing succeeded
pub struct SoapCallResult {
    pub status: ureq::http::StatusCode,
    pub raw_body: String,
    pub envelope: Option<SoapEnvelope>,
}

pub fn build_soap_body(
    action: &str,
    service_type: &str,
    args: &[(&str, &str)],
) -> Result<String, xmltree::Error> {
    build_soap_request(service_type, action, args)
}

/// Invoke a UPnP SOAP action on a control URL.
///
/// - `control_url`: full HTTP URL of the service control endpoint
/// - `service_type`: service URN
/// - `action`: action name
/// - `args`: list of (name, value)
pub fn invoke_upnp_action(
    control_url: &str,
    service_type: &str,
    action: &str,
    args: &[(&str, &str)],
) -> Result<SoapCallResult> {
    invoke_upnp_action_with_timeout(control_url, service_type, action, args, None)
}

pub fn invoke_upnp_action_with_timeout(
    control_url: &str,
    service_type: &str,
    action: &str,
    args: &[(&str, &str)],
    timeout: Option<Duration>,
) -> Result<SoapCallResult> {
    let body_xml =
        build_soap_body(action, service_type, args).context("Failed to build SOAP request body")?;

    let arg_log = summarize_args_for_log(args);
    debug!(
        url = control_url,
        action = action,
        service_type = service_type,
        args = ?arg_log,
        "Sending SOAP request"
    );

    trace!(body = body_xml.as_str(), "SOAP request body");

    let mut builder = Agent::config_builder();
    builder = builder.http_status_as_error(false);
    if let Some(duration) = timeout {
        builder = builder.timeout_global(Some(duration));
    }

    let config = builder.build();
    let agent: Agent = config.into();

    // 3. SOAPAction header
    let soap_action_header = format!(r#""{}#{}""#, service_type, action);

    // 4. HTTP POST
    let mut response = agent
        .post(control_url)
        .header("Content-Type", r#"text/xml; charset="utf-8""#)
        .header("SOAPAction", &soap_action_header)
        .send(body_xml)
        .with_context(|| format!("HTTP error when sending SOAP request to {}", control_url))?;

    let status = response.status();
    debug!(status = status.as_u16(), "SOAP response received");

    // 5. Read full body
    //
    //    API réelle (ureq 3.1.4):
    //    body_mut().read_to_string() -> Result<String, ureq::Error>
    let raw_body = response
        .body_mut()
        .read_to_string()
        .context("Failed to read SOAP response body")?;

    // 6. Try to parse SOAP envelope; non-fatal on failure
    let parsed_envelope = parse_soap_envelope(raw_body.as_bytes()).ok();

    if !status.is_success() {
        if is_oh_info_invalid_action(service_type, action, parsed_envelope.as_ref()) {
            debug!(
                url = control_url,
                action = action,
                service_type = service_type,
                status = status.as_u16(),
                "OpenHome Info action not supported (Invalid Action)"
            );
        } else {
            warn!(
                url = control_url,
                action = action,
                service_type = service_type,
                status = status.as_u16(),
                body_snippet = %response_snippet(&raw_body),
                "SOAP call returned non-success status"
            );
        }
    }

    Ok(SoapCallResult {
        status,
        raw_body,
        envelope: parsed_envelope,
    })
}

fn summarize_args_for_log<'a>(args: &'a [(&'a str, &'a str)]) -> Vec<String> {
    args.iter()
        .map(|(name, value)| format!("{}:{}B {}", name, value.len(), preview_value(value)))
        .collect()
}

fn preview_value(value: &str) -> String {
    const MAX_PREVIEW: usize = 96;
    if value.len() <= MAX_PREVIEW {
        value.to_string()
    } else {
        format!("{}…", &value[..MAX_PREVIEW])
    }
}

fn response_snippet(body: &str) -> String {
    const MAX_LEN: usize = 256;
    let trimmed = body.trim();
    if trimmed.len() <= MAX_LEN {
        trimmed.to_string()
    } else {
        format!("{}…", &trimmed[..MAX_LEN])
    }
}

fn is_oh_info_invalid_action(
    service_type: &str,
    action: &str,
    envelope: Option<&SoapEnvelope>,
) -> bool {
    if service_type != "urn:av-openhome-org:service:Info:1" {
        return false;
    }
    if action != "Id" && action != "TransportState" {
        return false;
    }
    let Some(env) = envelope else {
        return false;
    };
    match parse_upnp_error(env) {
        Some(err) if err.error_code == 401 => true,
        _ => false,
    }
}

#[derive(Debug, Clone)]
pub struct UpnpError {
    pub error_code: u32,
    pub error_description: String,
}

pub fn parse_upnp_error(envelope: &SoapEnvelope) -> Option<UpnpError> {
    let fault = find_child_with_suffix(&envelope.body.content, "Fault")?;
    let detail = find_child_with_suffix(fault, "detail")?;
    let upnp_error = find_child_with_suffix(detail, "UPnPError")?;

    let error_code_elem = upnp_error.children.iter().find_map(|node| match node {
        xmltree::XMLNode::Element(elem) if elem.name.ends_with("errorCode") => Some(elem),
        _ => None,
    })?;

    let error_code_text = error_code_elem.get_text()?.trim().to_string();
    let error_code = error_code_text.parse::<u32>().ok()?;

    let error_description = upnp_error
        .children
        .iter()
        .find_map(|node| match node {
            xmltree::XMLNode::Element(elem) if elem.name.ends_with("errorDescription") => {
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

fn find_child_with_suffix<'a>(
    parent: &'a xmltree::Element,
    suffix: &str,
) -> Option<&'a xmltree::Element> {
    parent.children.iter().find_map(|node| match node {
        xmltree::XMLNode::Element(elem) if elem.name.ends_with(suffix) => Some(elem),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::build_soap_body;

    #[test]
    fn build_body_preserves_openhome_argument_names() {
        let args = [
            ("AfterId", "0"),
            ("Uri", "http://example.test/audio.flac"),
            ("Metadata", "<DIDL-Lite/>"),
        ];
        let xml =
            build_soap_body("Insert", "urn:av-openhome-org:service:Playlist:1", &args).unwrap();
        assert!(xml.contains("<AfterId>0</AfterId>"));
        assert!(xml.contains("<Uri>http://example.test/audio.flac</Uri>"));
        assert!(xml.contains("<Metadata>"));
        assert!(xml.contains("</Metadata>"));
        assert!(xml.contains("DIDL-Lite"));
        assert!(!xml.contains("<aAfterId>"));
    }
}
