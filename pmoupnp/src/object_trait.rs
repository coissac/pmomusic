use xmltree::{Element, EmitterConfig};

use crate::UpnpObjectType;

pub trait UpnpXml {
    fn to_xml_element(&self) -> Element;
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
    fn to_markdown(&self) -> String {
        let elem = self.to_xml_element();
        let mut md = String::new();

        fn is_url(s: &str) -> bool {
            s.starts_with("http://") || s.starts_with("https://") || s.starts_with("urn:")
        }

        fn is_image_url(s: &str) -> bool {
            let s = s.to_lowercase();
            s.ends_with(".png")
                || s.ends_with(".jpg")
                || s.ends_with(".jpeg")
                || s.ends_with(".gif")
                || s.ends_with(".svg")
                || s.ends_with(".webp")
        }

        fn format_value(v: &str) -> String {
            let v = v.trim().to_string(); // <-- clone du texte nettoyé
            if is_url(&v) {
                if is_image_url(&v) {
                    format!("[{}]({})<br>![]({})", v, v, v)
                } else {
                    format!("[{}]({})", v, v)
                }
            } else {
                format!("`{}`", v)
            }
        }

        fn recurse(elem: &xmltree::Element, md: &mut String, depth: usize) {
            let indent = "  ".repeat(depth);
            md.push_str(&format!("{}- **{}**", indent, elem.name));

            // Attributs
            if !elem.attributes.is_empty() {
                let attrs: Vec<String> = elem
                    .attributes
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, format_value(v)))
                    .collect();
                md.push_str(&format!(" ({})", attrs.join(", ")));
            }

            // Texte (utilise get_text() maintenant)
            if let Some(text) = elem.get_text().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
                md.push_str(&format!(": {}", format_value(&text)));
            }

            md.push('\n');

            // Enfants
            for child in &elem.children {
                if let xmltree::XMLNode::Element(child_elem) = child {
                    recurse(child_elem, md, depth + 1);
                }
            }
        }

        md.push_str("# UPnP XML (Markdown view)\n\n");
        recurse(&elem, &mut md, 0);
        md
    }
}
pub trait UpnpObject: UpnpXml {
    fn as_upnp_object_type(&self) -> &UpnpObjectType;

    fn get_name(&self) -> &String {
        return &self.as_upnp_object_type().name;
    }

    fn get_object_type(&self) -> &String {
        &self.as_upnp_object_type().object_type
    }
}
