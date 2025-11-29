use anyhow::{anyhow, Result};
use crate::soap_client::invoke_upnp_action;
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
        response.children.push(XMLNode::Element(text_element("CurrentSpeed", "1")));

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
