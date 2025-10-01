use xmltree::{Element, XMLNode};

use crate::actions::Action;
use crate::actions::Argument;
use crate::actions::ArgumentSet;
use crate::actions::ActionInstance;
use crate::UpnpXml;
use crate::{UpnpObject, UpnpObjectType};

impl UpnpXml for ActionInstance {
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
impl UpnpObject for ActionInstance {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        return &self.object;
    }
}

impl ActionInstance {

    pub fn new(action: &Action) -> Self {
        Self {
            object: UpnpObjectType {
                name: action.get_name().clone(),
                object_type: "ActionInstance".to_string(),
            },
            model: action.clone(),
        }
    }

    pub fn arguments(&self, name: &str) -> Option<&Argument> {
        self.model.arguments.get(name)
    }

    pub fn arguments_set(&self) -> &ArgumentSet {
        &self.model.arguments
    }
}
