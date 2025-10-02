use std::collections::HashMap;

use tokio::sync::RwLock;
use xmltree::{Element, XMLNode};

use crate::{actions::{ArgInstanceSet, ArgumentSet}, state_variables::StateVariableSet, UpnpObject};

use crate::UpnpInstance;

impl UpnpObject for ArgInstanceSet {
    async fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("serviceStateTable");
        
        for state_var in self.all().await {
            let state_var_elem = state_var.to_xml_element().await; // retourne un <stateVariable> complet
            elem.children.push(XMLNode::Element(state_var_elem));
        }

        elem
    }
}

impl UpnpInstance for ArgInstanceSet {
    type Model = ArgumentSet;

    fn new(_: &ArgumentSet) -> Self {
        Self { objects: RwLock::new(HashMap::new()) }
    }
}


