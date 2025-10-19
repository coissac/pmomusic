use xmltree::{Element, XMLNode};

use crate::actions::{ActionInstanceSet, ActionSet};
use crate::{UpnpModel, UpnpObject};

impl UpnpObject for ActionSet {
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("actionList");

        for action in self.all() {
            let action_elem = action.to_xml_element(); // retourne un <action> complet
            elem.children.push(XMLNode::Element(action_elem));
        }

        elem
    }
}

impl UpnpModel for ActionSet {
    type Instance = ActionInstanceSet;
}
