//! Implémentation des traits UPnP pour Service.

use xmltree::{Element, XMLNode};

use crate::{
    services::{Service, ServiceInstance},
    UpnpObject, UpnpModel, UpnpTyped, UpnpObjectType,
};

impl std::fmt::Display for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Service({}:{})", self.name(), self.version())
    }
}

impl UpnpTyped for Service {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        &self.object
    }
}

impl UpnpObject for Service {
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("service");

        // serviceType
        let mut service_type = Element::new("serviceType");
        service_type.children.push(XMLNode::Text(self.service_type()));
        elem.children.push(XMLNode::Element(service_type));

        // serviceId
        let mut service_id = Element::new("serviceId");
        service_id.children.push(XMLNode::Text(self.service_id()));
        elem.children.push(XMLNode::Element(service_id));

        // SCPDURL
        let mut SCPDURL = Element::new("SCPDURL");
        SCPDURL.children.push(XMLNode::Text(self.scpd_url()));
        elem.children.push(XMLNode::Element(SCPDURL));

        // controlURL
        let mut controlURL = Element::new("controlURL");
        controlURL.children.push(XMLNode::Text(self.control_url()));
        elem.children.push(XMLNode::Element(controlURL));

        // SCPDURL
        let mut eventSubURL = Element::new("eventSubURL");
        eventSubURL.children.push(XMLNode::Text(self.event_url()));
        elem.children.push(XMLNode::Element(eventSubURL));

        elem
    }
}

impl UpnpModel for Service {
    type Instance = ServiceInstance;
}