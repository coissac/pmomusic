//! Parser SOAP pour actions UPnP

use super::{SoapBody, SoapEnvelope, SoapHeader};
use std::collections::HashMap;
use std::io::BufReader;
use xmltree::Element;

/// Action UPnP extraite d'une enveloppe SOAP
#[derive(Debug, Clone)]
pub struct SoapAction {
    /// Nom de l'action (ex: "Play", "SetAVTransportURI")
    pub name: String,

    /// Namespace de l'action (ex: "urn:schemas-upnp-org:service:AVTransport:1")
    pub namespace: Option<String>,

    /// Arguments de l'action
    pub args: HashMap<String, String>,
}

/// Erreur de parsing SOAP
#[derive(Debug, thiserror::Error)]
pub enum SoapParseError {
    #[error("XML parse error: {0}")]
    XmlError(#[from] xmltree::ParseError),

    #[error("Missing SOAP Envelope")]
    MissingEnvelope,

    #[error("Missing SOAP Body")]
    MissingBody,

    #[error("No action found in SOAP Body")]
    NoAction,
}

/// Parse une action SOAP à partir de bytes XML
pub fn parse_soap_action(xml: &[u8]) -> Result<SoapAction, SoapParseError> {
    let envelope = parse_soap_envelope(xml)?;
    extract_action_from_body(&envelope.body)
}

/// Parse une enveloppe SOAP complète
pub fn parse_soap_envelope(xml: &[u8]) -> Result<SoapEnvelope, SoapParseError> {
    let reader = BufReader::new(xml);
    let root = Element::parse(reader)?;

    // Vérifier que c'est bien une Envelope
    if !root.name.ends_with("Envelope") {
        return Err(SoapParseError::MissingEnvelope);
    }

    // Extraire Header (optionnel)
    let header = root
        .get_child("Header")
        .or_else(|| root.children.iter().find_map(|n| n.as_element()))
        .filter(|e| e.name.ends_with("Header"))
        .map(|e| SoapHeader { content: e.clone() });

    // Extraire Body (obligatoire)
    let body_elem = root
        .get_child("Body")
        .or_else(|| {
            root.children
                .iter()
                .find_map(|n| n.as_element().filter(|e| e.name.ends_with("Body")))
        })
        .ok_or(SoapParseError::MissingBody)?;

    let body = SoapBody {
        content: body_elem.clone(),
    };

    Ok(SoapEnvelope { header, body })
}

/// Extrait l'action UPnP du corps SOAP
fn extract_action_from_body(body: &SoapBody) -> Result<SoapAction, SoapParseError> {
    // Le Body contient un élément enfant qui est l'action
    // Format: <u:ActionName xmlns:u="service-urn">...</u:ActionName>

    let action_elem = body
        .content
        .children
        .iter()
        .find_map(|n| n.as_element())
        .ok_or(SoapParseError::NoAction)?;

    let name = action_elem.name.clone();
    let namespace = action_elem.namespace.clone();

    // Extraire les arguments (enfants directs de l'action)
    let mut args = HashMap::new();
    for child in &action_elem.children {
        if let Some(elem) = child.as_element() {
            let arg_name = elem.name.clone();
            let arg_value = elem.get_text().unwrap_or_default().to_string();
            args.insert(arg_name, arg_value);
        }
    }

    Ok(SoapAction {
        name,
        namespace,
        args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_action() {
        let xml = r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <u:Play xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
      <Speed>1</Speed>
    </u:Play>
  </s:Body>
</s:Envelope>"#;

        let action = parse_soap_action(xml.as_bytes()).unwrap();
        assert_eq!(action.name, "Play");
        assert_eq!(
            action.namespace,
            Some("urn:schemas-upnp-org:service:AVTransport:1".to_string())
        );
        assert_eq!(action.args.get("InstanceID"), Some(&"0".to_string()));
        assert_eq!(action.args.get("Speed"), Some(&"1".to_string()));
    }

    #[test]
    fn test_parse_action_no_args() {
        let xml = r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <u:Stop xmlns:u="urn:schemas-upnp-org:service:AVTransport:1"/>
  </s:Body>
</s:Envelope>"#;

        let action = parse_soap_action(xml.as_bytes()).unwrap();
        assert_eq!(action.name, "Stop");
        assert!(action.args.is_empty());
    }
}
