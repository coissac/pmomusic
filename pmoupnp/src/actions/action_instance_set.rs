use crate::{
    UpnpTypedObject, UpnpObject,
    actions::{ActionInstanceSet},
};

use xmltree::{Element,XMLNode};

impl UpnpObject for ActionInstanceSet {
    // Méthode pour convertir en XML (à implémenter avec une librairie XML)
    async fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("actionList");

        for action in self.all().await {
            let action_elem = action.to_xml_element().await; // retourne un <action> complet
            elem.children.push(XMLNode::Element(action_elem));
        }

        elem
    }
}

