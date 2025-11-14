//! Implémentation des traits UPnP pour Service.
//!
//! Ce module fournit les implémentations des traits principaux du framework
//! UPnP pour le type [`Service`]:
//!
//! - [`Display`] : Affichage formaté d'un service
//! - [`UpnpTyped`] : Accès aux métadonnées de type UPnP
//! - [`UpnpObject`] : Sérialisation XML pour la description de device
//! - [`UpnpModel`] : Association du modèle avec son type d'instance
//!
//! Ces implémentations permettent aux services de s'intégrer dans
//! l'architecture UPnP générique du framework.

use xmltree::{Element, XMLNode};

use crate::{
    UpnpModel, UpnpObject, UpnpObjectType, UpnpTyped,
    services::{Service, ServiceInstance},
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
        service_type
            .children
            .push(XMLNode::Text(self.service_type()));
        elem.children.push(XMLNode::Element(service_type));

        // serviceId
        let mut service_id = Element::new("serviceId");
        service_id.children.push(XMLNode::Text(self.service_id()));
        elem.children.push(XMLNode::Element(service_id));

        // SCPDURL
        let mut scpdurl = Element::new("SCPDURL");
        scpdurl.children.push(XMLNode::Text(self.scpd_route()));
        elem.children.push(XMLNode::Element(scpdurl));

        // controlURL
        let mut control_url = Element::new("controlURL");
        control_url
            .children
            .push(XMLNode::Text(self.control_route()));
        elem.children.push(XMLNode::Element(control_url));

        // eventSubURL
        let mut event_sub_url = Element::new("eventSubURL");
        event_sub_url
            .children
            .push(XMLNode::Text(self.event_route()));
        elem.children.push(XMLNode::Element(event_sub_url));

        elem
    }
}

impl UpnpModel for Service {
    type Instance = ServiceInstance;
}
