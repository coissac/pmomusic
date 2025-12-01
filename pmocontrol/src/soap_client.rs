use std::time::Duration;

use anyhow::{Context, Result};
use pmoupnp::soap::{SoapEnvelope, build_soap_request, parse_soap_envelope};
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
    let body_xml = build_soap_request(service_type, action, args)
        .context("Failed to build SOAP request body")?;

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

    // 5. Read full body
    //
    //    API réelle (ureq 3.1.4):
    //    body_mut().read_to_string() -> Result<String, ureq::Error>
    let raw_body = response
        .body_mut()
        .read_to_string()
        .context("Failed to read SOAP response body")?;

    // 6. Try to parse SOAP envelope; non-fatal on failure
    let envelope = match parse_soap_envelope(raw_body.as_bytes()) {
        Ok(env) => Some(env),
        Err(_) => None,
    };

    Ok(SoapCallResult {
        status,
        raw_body,
        envelope,
    })
}
