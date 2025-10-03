use crate::actions::ArgInstanceSet;
use crate::UpnpModel;
use crate::{
    UpnpObject,
    actions::{ArgumentSet},
};
use xmltree::Element;

impl UpnpObject for ArgumentSet {
    // Méthode pour convertir en XML (à implémenter avec une librairie XML)
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("argumentList");

        for arg in self.all() {
            let arg_elem = arg.to_xml_element(); // toujours un <argumentList> contenant 1 ou 2 <argument>

            // Pour InOut, on ajoute tous les enfants du <argumentList> généré
            for child in arg_elem.children {
                elem.children.push(child);
            }
        }

        elem
    }
}

impl UpnpModel for ArgumentSet {
    type Instance = ArgInstanceSet;
}
