use std::collections::HashMap;

use std::sync::RwLock;
use xmltree::{Element, XMLNode};

use crate::{
    UpnpObject,
    actions::{ArgInstanceSet, ArgumentSet},
};

use crate::UpnpInstance;

impl UpnpObject for ArgInstanceSet {
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("serviceStateTable");

        for state_var in self.all() {
            let state_var_elem = state_var.to_xml_element(); // retourne un <stateVariable> complet
            elem.children.push(XMLNode::Element(state_var_elem));
        }

        elem
    }
}

impl UpnpInstance for ArgInstanceSet {
    type Model = ArgumentSet;

    fn new(_: &ArgumentSet) -> Self {
        Self {
            objects: RwLock::new(HashMap::new()),
        }
    }
}
