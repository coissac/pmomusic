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
/// - `service_type`: service URN, e.g. "urn:schemas-upnp-org:service:AVTransport:1"
/// - `action`: action name, e.g. "GetTransportInfo"
/// - `args`: list of (name, value) pairs, e.g. &[("InstanceID", "0")]
pub fn invoke_upnp_action(
    control_url: &str,
    service_type: &str,
    action: &str,
    args: &[(&str, &str)],
) -> Result<SoapCallResult> {
    // 1. Build SOAP request body using pmoupnp::soap
    let body_xml = build_soap_request(service_type, action, args)
        .context("Failed to build SOAP request body")?;

    // 2. Build an Agent config that does NOT treat 4xx/5xx as errors.
    //
    //    This is crucial: we want to be able to read the body even for
    //    HTTP 500 SOAP Faults, so we must *not* get Error::StatusCode.
    let config = Agent::config_builder()
        .http_status_as_error(false)
        .build();

    let agent: Agent = config.into();

    // 3. Build SOAPAction header: "urn:service#Action"
    let soap_action_header = format!(r#""{}#{}""#, service_type, action);

    // 4. Send HTTP POST request
    //
    //    - RequestBuilder::header(...) is the proper 3.x API.
    //    - RequestBuilder::send(...) accepts anything implementing AsSendBody,
    //      including `String`.
    let mut response = agent
        .post(control_url)
        .header("Content-Type", r#"text/xml; charset="utf-8""#)
        .header("SOAPAction", &soap_action_header)
        .send(body_xml)
        .with_context(|| format!("HTTP error when sending SOAP request to {}", control_url))?;

    let status = response.status();

    // 5. Read full body into a String, regardless of HTTP status code.
    //
    //    This matches the pattern you already use for description.xml:
    //    response.body_mut().read_to_string(...)
    let raw_body = String::new();
    response
        .body_mut()
        .read_to_string()
        .context("Failed to read SOAP response body")?;

    // 6. Try to parse SOAP envelope.
    //
    //    We *do not* fail the whole call if parsing fails:
    //    - you still get status + raw_body,
    //    - envelope is None if body is not valid SOAP.
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
