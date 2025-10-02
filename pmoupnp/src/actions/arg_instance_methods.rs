use xmltree::Element;

use crate::{actions::{Argument, ArgumentInstance}, UpnpInstance, UpnpObject, UpnpObjectType, UpnpTyped, UpnpTypedInstance};


impl UpnpObject for ArgumentInstance {
    async fn to_xml_element(&self) -> Element {
        self.get_model().to_xml_element().await
    }
}

impl UpnpTyped for ArgumentInstance {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        return &self.object;
    }
}

impl UpnpTypedInstance for ArgumentInstance {

    fn get_model(&self) -> &Self::Model {
        &self.model
    }
}



impl UpnpInstance for ArgumentInstance {
    type Model = Argument;

    fn new(from: &Argument) -> Self {
        Self {
            object: UpnpObjectType {
                name: from.get_name().clone(),
                object_type: "UpnpInstance".to_string(),
            },

            model: from.clone(),
            variable_instance: None,
         }
    }

}