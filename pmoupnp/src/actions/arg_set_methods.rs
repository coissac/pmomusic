use crate::UpnpModel;
use crate::actions::ArgInstanceSet;
use crate::{UpnpObject, actions::ArgumentSet};
use xmltree::{Element, XMLNode};

impl UpnpObject for ArgumentSet {
    // Méthode pour convertir en XML (à implémenter avec une librairie XML)
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("argumentList");

        for arg in self.all() {
            for arg_elem in arg.to_xml_elements() {
                elem.children.push(XMLNode::Element(arg_elem));
            }
        }

        elem
    }
}

impl UpnpModel for ArgumentSet {
    type Instance = ArgInstanceSet;
}
