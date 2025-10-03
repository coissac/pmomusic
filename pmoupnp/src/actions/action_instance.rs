use std::sync::Arc;

use xmltree::{Element, XMLNode};

use crate::actions::Action;
use crate::actions::Argument;
use crate::actions::ArgumentSet;
use crate::actions::ActionInstance;
use crate::UpnpInstance;
use crate::UpnpObject;
use crate::UpnpTyped;
use crate::UpnpTypedInstance;
use crate::UpnpObjectType;

impl UpnpObject for ActionInstance {
fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("action");

        // <name>
        let mut name_elem = Element::new("name");
        name_elem.children.push(XMLNode::Text(self.get_name().clone()));
        elem.children.push(XMLNode::Element(name_elem));

        // déplacer tous les enfants de args_elem dans un nouvel Element
        let args_container = self.arguments_set().to_xml_element();
        elem.children.push(XMLNode::Element(args_container));

        elem
    }  
}

impl UpnpTyped for ActionInstance {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        return &self.object;
    }
}

impl UpnpInstance for ActionInstance {

    type Model = Action;

    fn new(action: &Action) -> Self {
        Self {
            object: UpnpObjectType {
                name: action.get_name().clone(),
                object_type: "ActionInstance".to_string(),
            },
            model: action.clone(),
        }
    }

}


impl UpnpTypedInstance for ActionInstance {

    fn get_model(&self) -> &Self::Model {
        &self.model
    }
}

impl ActionInstance {


    pub fn arguments(&self, name: &str) -> Option<Arc<Argument>> {
        self.model.arguments.get_by_name(name)
    }

    pub fn arguments_set(&self) -> &ArgumentSet {
        &self.model.arguments
    }
}
