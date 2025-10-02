use std::collections::HashMap;

use tokio::sync::RwLock;
use xmltree::{Element, XMLNode};

use crate::{state_variables::{StateVarInstance, StateVarInstanceSet, StateVariableSet}, UpnInstanceSet, UpnpObject, UpnpTyped};

use crate::UpnpInstance;

impl UpnpObject for StateVarInstanceSet {
    async fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("serviceStateTable");
        
        for state_var in self.all().await {
            let state_var_elem = state_var.to_xml_element().await; // retourne un <stateVariable> complet
            elem.children.push(XMLNode::Element(state_var_elem));
        }

        elem
    }
}

impl UpnpInstance for StateVarInstanceSet {
    type Model = StateVariableSet;

    fn new(_: &StateVariableSet) -> Self {
        Self { objects: RwLock::new(HashMap::new()) }
    }


}


