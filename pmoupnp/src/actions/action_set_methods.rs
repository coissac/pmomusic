use std::collections::HashMap;
use xmltree::{Element, XMLNode};

use crate::actions::Action;
use crate::actions::ActionSet;
use crate::actions::errors::ActionError;
use crate::UpnpObject;
use crate::UpnpXml;

impl UpnpXml for ActionSet {
         fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("actionList");

        for action in self.iter() {
            let action_elem = action.to_xml_element(); // retourne un <action> complet
            elem.children.push(XMLNode::Element(action_elem));
        }

        elem
    }

}

impl ActionSet {
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }

    pub fn insert(&mut self, action: Action) -> Result<(), ActionError> {
        let name = action.get_name();
        if self.actions.contains_key(name) {
            return Err(ActionError::SetError(
                format!("Action {} already exists", name)
            ));
        }
        self.actions.insert(name.clone(), action);
        Ok(())
    }

    pub fn insert_or_replace(&mut self, action: Action) {
        self.actions.insert(action.get_name().clone(), action);
    }

    pub fn contains(&self, name: &str) -> bool {
        self.actions.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&Action> {
        self.actions.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Action> {
        self.actions.values()
    }

    pub fn all(&self) -> Vec<&Action> {
        self.actions.values().collect()
    }
}
