use xmltree::{Element, XMLNode};

use crate::{object_trait::UpnpModel, state_variables::{StateVarInstanceSet, StateVariableSet}, UpnInstanceSet, UpnpObject};


impl UpnpObject for StateVariableSet {

    async fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("serviceStateTable");
        
        for state_var in self.get_all() {
            let state_var_elem = state_var.to_xml_element().await; // retourne un <stateVariable> complet
            elem.children.push(XMLNode::Element(state_var_elem));
        }

        elem
    }

}

impl UpnpModel for StateVariableSet {
    type Instance = StateVarInstanceSet;
}


