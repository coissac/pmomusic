use crate::UpnpObject;
use crate::{
    UpnpXml,
    actions::{Argument, ArgumentSet},
};
use std::collections::HashMap;
use xmltree::Element;

impl UpnpXml for ArgumentSet {
    // Méthode pour convertir en XML (à implémenter avec une librairie XML)
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("argumentList");

        for arg in self.iter() {
            let arg_elem = arg.to_xml_element(); // toujours un <argumentList> contenant 1 ou 2 <argument>

            // Pour InOut, on ajoute tous les enfants du <argumentList> généré
            for child in arg_elem.children {
                elem.children.push(child);
            }
        }

        elem
    }
}
impl ArgumentSet {
    pub fn new() -> Self {
        Self {
            arguments: HashMap::new(),
        }
    }

    pub fn insert(&mut self, arg: Argument) {
        self.arguments.insert(arg.get_name().clone(), arg);
    }

    pub fn contains(&self, name: &str) -> bool {
        self.arguments.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&Argument> {
        self.arguments.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Argument> {
        self.arguments.values()
    }

    pub fn all(&self) -> Vec<&Argument> {
        self.arguments.values().collect()
    }
}
