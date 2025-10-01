use xmltree::{Element, EmitterConfig};

use crate::UpnpObjectType;

pub trait UpnpObject {
    fn as_upnp_object_type(&self) -> &UpnpObjectType;
    fn to_xml_element(&self) -> Element;

    fn get_name(&self) -> &String {
        return &self.as_upnp_object_type().name;
    }

    fn get_object_type(&self) -> &String {
        &self.as_upnp_object_type().object_type
    }

    fn to_xml(&self) -> String {
        let elem = self.to_xml_element();

        // Configurer l'indentation
        let config = EmitterConfig::new()
            .perform_indent(true)
            .indent_string("  "); // 2 espaces

        // Sérialiser dans un buffer
        let mut buf = Vec::new();
        // écrire l'élément
        elem.write_with_config(&mut buf, config)
            .expect("Failed to write XML");

        // Préfixer avec l'en-tête XML
        let mut xml_string = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n".to_string();
        xml_string.push_str(&String::from_utf8(buf).expect("Invalid UTF-8"));

        xml_string
    }
}
