//! SOAP Faults pour UPnP

use xmltree::{Element, XMLNode};

/// Erreur SOAP (Fault)
#[derive(Debug, Clone)]
pub struct SoapFault {
    /// Code d'erreur (ex: "s:Client", "401")
    pub fault_code: String,

    /// Description de l'erreur
    pub fault_string: String,

    /// Détails UPnP optionnels
    pub upnp_error: Option<UpnpError>,
}

/// Erreur UPnP spécifique
#[derive(Debug, Clone)]
pub struct UpnpError {
    /// Code d'erreur UPnP (ex: "401", "501")
    pub error_code: String,

    /// Description de l'erreur
    pub error_description: String,
}

impl SoapFault {
    /// Crée un fault SOAP simple
    pub fn new(fault_code: String, fault_string: String) -> Self {
        Self {
            fault_code,
            fault_string,
            upnp_error: None,
        }
    }

    /// Crée un fault SOAP avec erreur UPnP
    pub fn with_upnp_error(
        fault_code: String,
        fault_string: String,
        error_code: String,
        error_description: String,
    ) -> Self {
        Self {
            fault_code,
            fault_string,
            upnp_error: Some(UpnpError {
                error_code,
                error_description,
            }),
        }
    }
}

/// Construit un SOAP Fault XML
///
/// # Arguments
///
/// * `fault_code` - Code du fault (ex: "s:Client")
/// * `fault_string` - Message d'erreur
/// * `upnp_error_code` - Code d'erreur UPnP optionnel (ex: "401")
/// * `upnp_error_desc` - Description d'erreur UPnP optionnelle
///
/// # Returns
///
/// XML SOAP Fault formaté
pub fn build_soap_fault(
    fault_code: &str,
    fault_string: &str,
    upnp_error_code: Option<&str>,
    upnp_error_desc: Option<&str>,
) -> Result<String, xmltree::Error> {
    // Construire l'élément Fault
    let mut fault = Element::new("s:Fault");

    // faultcode
    let mut faultcode_elem = Element::new("faultcode");
    faultcode_elem
        .children
        .push(XMLNode::Text(fault_code.to_string()));
    fault.children.push(XMLNode::Element(faultcode_elem));

    // faultstring
    let mut faultstring_elem = Element::new("faultstring");
    faultstring_elem
        .children
        .push(XMLNode::Text(fault_string.to_string()));
    fault.children.push(XMLNode::Element(faultstring_elem));

    // detail (si erreur UPnP)
    if let (Some(code), Some(desc)) = (upnp_error_code, upnp_error_desc) {
        let mut detail = Element::new("detail");

        let mut upnp_error = Element::new("UPnPError");
        upnp_error.attributes.insert(
            "xmlns".to_string(),
            "urn:schemas-upnp-org:control-1-0".to_string(),
        );

        let mut error_code_elem = Element::new("errorCode");
        error_code_elem
            .children
            .push(XMLNode::Text(code.to_string()));
        upnp_error
            .children
            .push(XMLNode::Element(error_code_elem));

        let mut error_desc_elem = Element::new("errorDescription");
        error_desc_elem
            .children
            .push(XMLNode::Text(desc.to_string()));
        upnp_error
            .children
            .push(XMLNode::Element(error_desc_elem));

        detail.children.push(XMLNode::Element(upnp_error));
        fault.children.push(XMLNode::Element(detail));
    }

    // Construire le Body
    let mut body = Element::new("s:Body");
    body.children.push(XMLNode::Element(fault));

    // Construire l'Envelope
    let mut envelope = Element::new("s:Envelope");
    envelope.attributes.insert(
        "xmlns:s".to_string(),
        "http://schemas.xmlsoap.org/soap/envelope/".to_string(),
    );
    envelope.children.push(XMLNode::Element(body));

    // Sérialiser
    let mut buf = Vec::new();
    let config = xmltree::EmitterConfig::new()
        .perform_indent(true)
        .indent_string("  ");
    envelope.write_with_config(&mut buf, config)?;

    Ok(String::from_utf8(buf).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_simple_fault() {
        let xml = build_soap_fault("s:Client", "Invalid Action", None, None).unwrap();

        assert!(xml.contains("<s:Fault>"));
        assert!(xml.contains("<faultcode>s:Client</faultcode>"));
        assert!(xml.contains("<faultstring>Invalid Action</faultstring>"));
        assert!(!xml.contains("UPnPError"));
    }

    #[test]
    fn test_build_upnp_fault() {
        let xml = build_soap_fault(
            "s:Client",
            "UPnP Error",
            Some("401"),
            Some("Invalid Action"),
        )
        .unwrap();

        assert!(xml.contains("<s:Fault>"));
        assert!(xml.contains("<detail>"));
        assert!(xml.contains("<UPnPError"));
        assert!(xml.contains("<errorCode>401</errorCode>"));
        assert!(xml.contains("<errorDescription>Invalid Action</errorDescription>"));
    }
}
