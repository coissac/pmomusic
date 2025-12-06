//! Construction de réponses SOAP

use xmltree::{Element, XMLNode};

fn build_soap_envelope_with_body(body_child: Element) -> Result<String, xmltree::Error> {
    // Body
    let mut body = Element::new("s:Body");
    body.children.push(XMLNode::Element(body_child));

    // Envelope
    let mut envelope = Element::new("s:Envelope");
    envelope.attributes.insert(
        "xmlns:s".to_string(),
        "http://schemas.xmlsoap.org/soap/envelope/".to_string(),
    );
    envelope.attributes.insert(
        "s:encodingStyle".to_string(),
        "http://schemas.xmlsoap.org/soap/encoding/".to_string(),
    );
    envelope.children.push(XMLNode::Element(body));

    let mut buf = Vec::new();
    let config = xmltree::EmitterConfig::new()
        .write_document_declaration(true)
        .perform_indent(true)
        .indent_string("  ");
    envelope.write_with_config(&mut buf, config)?;

    Ok(String::from_utf8(buf).unwrap())
}

/// Construit une réponse SOAP UPnP
///
/// # Arguments
///
/// * `service_urn` - URN du service (ex: "urn:schemas-upnp-org:service:AVTransport:1")
/// * `action` - Nom de l'action (ex: "GetPositionInfo")
/// * `values` - Map des valeurs de retour
///
/// # Returns
///
/// XML SOAP formaté en String
pub fn build_soap_response(
    service_urn: &str,
    action: &str,
    values: Vec<(String, String)>,
) -> Result<String, xmltree::Error> {
    let response_name = format!("u:{}Response", action);
    let mut response_elem = Element::new(&response_name);
    response_elem
        .attributes
        .insert("xmlns:u".to_string(), service_urn.to_string());

    for (key, value) in values {
        let mut child = Element::new(&key);
        child.children.push(XMLNode::Text(value));
        response_elem.children.push(XMLNode::Element(child));
    }

    build_soap_envelope_with_body(response_elem)
}

pub fn build_soap_request(
    service_urn: &str,
    action: &str,
    args: &[(&str, &str)],
) -> Result<String, xmltree::Error> {
    let request_name = format!("u:{}", action);
    let mut request_elem = Element::new(&request_name);
    request_elem
        .attributes
        .insert("xmlns:u".to_string(), service_urn.to_string());

    for (name, value) in args {
        let mut child = Element::new(*name);
        child.children.push(XMLNode::Text((*value).to_string()));
        request_elem.children.push(XMLNode::Element(child));
    }

    build_soap_envelope_with_body(request_elem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_response() {
        let mut values = Vec::new();
        values.push(("Track".to_string(), "5".to_string()));
        values.push(("TrackDuration".to_string(), "00:03:45".to_string()));

        let xml = build_soap_response(
            "urn:schemas-upnp-org:service:AVTransport:1",
            "GetPositionInfo",
            values,
        )
        .unwrap();

        assert!(xml.contains("GetPositionInfoResponse"));
        assert!(xml.contains("<Track>5</Track>"));
        assert!(xml.contains("<TrackDuration>00:03:45</TrackDuration>"));
        assert!(xml.contains("xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\""));
    }

    #[test]
    fn test_build_empty_response() {
        let values = Vec::new();

        let xml = build_soap_response("urn:schemas-upnp-org:service:AVTransport:1", "Stop", values)
            .unwrap();

        assert!(xml.contains("StopResponse"));
        assert!(xml.contains("xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\""));
    }
}
