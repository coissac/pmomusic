//! Implémentation des traits UPnP pour Device.

use xmltree::{Element, XMLNode};

use crate::{
    devices::{Device, DeviceInstance},
    UpnpObject, UpnpModel,
};

impl UpnpObject for Device {
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("device");

        // deviceType
        let mut device_type = Element::new("deviceType");
        device_type.children.push(XMLNode::Text(self.device_type()));
        elem.children.push(XMLNode::Element(device_type));

        // friendlyName
        let mut friendly_name = Element::new("friendlyName");
        friendly_name.children.push(XMLNode::Text(self.friendly_name().to_string()));
        elem.children.push(XMLNode::Element(friendly_name));

        // manufacturer
        let mut manufacturer = Element::new("manufacturer");
        manufacturer.children.push(XMLNode::Text(self.manufacturer().to_string()));
        elem.children.push(XMLNode::Element(manufacturer));

        // manufacturerURL (optionnel)
        if let Some(url) = self.manufacturer_url() {
            let mut manufacturer_url = Element::new("manufacturerURL");
            manufacturer_url.children.push(XMLNode::Text(url.to_string()));
            elem.children.push(XMLNode::Element(manufacturer_url));
        }

        // modelDescription (optionnel)
        if let Some(desc) = self.model_description() {
            let mut model_description = Element::new("modelDescription");
            model_description.children.push(XMLNode::Text(desc.to_string()));
            elem.children.push(XMLNode::Element(model_description));
        }

        // modelName
        let mut model_name = Element::new("modelName");
        model_name.children.push(XMLNode::Text(self.model_name().to_string()));
        elem.children.push(XMLNode::Element(model_name));

        // modelNumber (optionnel)
        if let Some(number) = self.model_number() {
            let mut model_number = Element::new("modelNumber");
            model_number.children.push(XMLNode::Text(number.to_string()));
            elem.children.push(XMLNode::Element(model_number));
        }

        // modelURL (optionnel)
        if let Some(url) = self.model_url() {
            let mut model_url = Element::new("modelURL");
            model_url.children.push(XMLNode::Text(url.to_string()));
            elem.children.push(XMLNode::Element(model_url));
        }

        // serialNumber (optionnel)
        if let Some(serial) = self.serial_number() {
            let mut serial_number = Element::new("serialNumber");
            serial_number.children.push(XMLNode::Text(serial.to_string()));
            elem.children.push(XMLNode::Element(serial_number));
        }

        // UPC (optionnel)
        if let Some(upc) = self.upc() {
            let mut upc_elem = Element::new("UPC");
            upc_elem.children.push(XMLNode::Text(upc.to_string()));
            elem.children.push(XMLNode::Element(upc_elem));
        }

        // iconList (optionnel)
        if let Some(icon_url) = self.icon_url() {
            let mut icon_list = Element::new("iconList");
            let mut icon = Element::new("icon");

            let mut mimetype = Element::new("mimetype");
            mimetype.children.push(XMLNode::Text("image/png".to_string()));
            icon.children.push(XMLNode::Element(mimetype));

            let mut width = Element::new("width");
            width.children.push(XMLNode::Text("48".to_string()));
            icon.children.push(XMLNode::Element(width));

            let mut height = Element::new("height");
            height.children.push(XMLNode::Text("48".to_string()));
            icon.children.push(XMLNode::Element(height));

            let mut depth = Element::new("depth");
            depth.children.push(XMLNode::Text("24".to_string()));
            icon.children.push(XMLNode::Element(depth));

            let mut url = Element::new("url");
            url.children.push(XMLNode::Text(icon_url.to_string()));
            icon.children.push(XMLNode::Element(url));

            icon_list.children.push(XMLNode::Element(icon));
            elem.children.push(XMLNode::Element(icon_list));
        }

        // presentationURL (optionnel)
        if let Some(url) = self.presentation_url() {
            let mut presentation_url = Element::new("presentationURL");
            presentation_url.children.push(XMLNode::Text(url.to_string()));
            elem.children.push(XMLNode::Element(presentation_url));
        }

        elem
    }
}

impl UpnpModel for Device {
    type Instance = DeviceInstance;
}
