use std::sync::Arc;

use xmltree::{Element,XMLNode};

use crate::actions::ActionInstance;
use crate::UpnpModel;
use crate::UpnpObject;
use crate::actions::Action;
use crate::actions::Argument;
use crate::actions::ArgumentSet;
use crate::UpnpTyped;
use crate::{UpnpTypedObject, UpnpObjectType};

impl UpnpObject for Action {
    async fn to_xml_element(&self) -> Element {
        let mut action_elem = Element::new("action");

        // <name>
        let mut name_elem = Element::new("name");
        name_elem
            .children
            .push(XMLNode::Text(self.get_name().clone()));
        action_elem.children.push(XMLNode::Element(name_elem));

        // <argumentList>
        let args_elem = self.arguments.to_xml_element().await;
        action_elem.children.push(XMLNode::Element(args_elem));

        action_elem
    }
}

impl UpnpModel for Action {

    type Instance = ActionInstance;
}


impl UpnpTyped for Action {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        return &self.object;
    }
}

impl Action {
    pub fn new(name: String) -> Action {
        Self {
            object: UpnpObjectType {
                name,
                object_type: "Action".to_string(),
            },
            arguments: ArgumentSet::new(),
        }
    }

    pub fn add_argument(&mut self, arg: Arc<Argument>) {
        self.arguments.insert(arg);
    }

    pub fn arguments(&self) -> &ArgumentSet {
        &self.arguments
    }
}
