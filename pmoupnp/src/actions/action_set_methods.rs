use std::collections::HashMap;
use xmltree::{Element, XMLNode};

use crate::actions::Action;
use crate::actions::ActionSet;
use crate::actions::errors::ActionError;
use crate::UpnpTypedObject;
use crate::UpnpObject;

impl UpnpObject for ActionSet {
         async fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("actionList");

        for action in self.all().await {
            let action_elem = action.to_xml_element().await; // retourne un <action> complet
            elem.children.push(XMLNode::Element(action_elem));
        }

        elem
    }

}

