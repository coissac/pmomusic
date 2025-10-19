use std::collections::HashMap;

use std::sync::RwLock;
use xmltree::{Element, XMLNode};

use crate::{
    UpnpObject,
    state_variables::{StateVarInstanceSet, StateVariableSet},
};

use crate::UpnpInstance;

impl UpnpObject for StateVarInstanceSet {
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("serviceStateTable");

        for state_var in self.all() {
            let state_var_elem = state_var.to_xml_element(); // retourne un <stateVariable> complet
            elem.children.push(XMLNode::Element(state_var_elem));
        }

        elem
    }
}

impl UpnpInstance for StateVarInstanceSet {
    type Model = StateVariableSet;

    fn new(_: &StateVariableSet) -> Self {
        Self {
            objects: RwLock::new(HashMap::new()),
        }
    }
}
