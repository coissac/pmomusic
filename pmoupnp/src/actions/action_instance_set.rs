use crate::{
    UpnpObject, UpnpXml,
    actions::{ActionInstance, ActionInstanceSet},
};
use std::collections::HashMap;
use xmltree::{Element,XMLNode};

impl UpnpXml for ActionInstanceSet {
    // Méthode pour convertir en XML (à implémenter avec une librairie XML)
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("actionList");

        for action in self.iter() {
            let action_elem = action.to_xml_element(); // retourne un <action> complet
            elem.children.push(XMLNode::Element(action_elem));
        }

        elem
    }
}

impl ActionInstanceSet {
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
        }
    }

    pub fn insert(&mut self, instance: ActionInstance) {
        self.instances.insert(instance.get_name().clone(), instance);
    }

    pub fn contains(&self, name: &str) -> bool {
        self.instances.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&ActionInstance> {
        self.instances.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ActionInstance> {
        self.instances.values()
    }

    pub fn all(&self) -> Vec<&ActionInstance> {
        self.instances.values().collect()
    }
}

