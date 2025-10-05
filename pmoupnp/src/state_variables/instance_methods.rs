use chrono::{DateTime, Utc};
use xmltree::Element;

use crate::{
    state_variables::{StateVarInstance, StateVariable, UpnpVariable}, 
    variable_types::StateValue, 
    UpnpObject, 
    UpnpObjectType
};

impl UpnpVariable for StateVarInstance {
    fn get_definition(&self) -> &StateVariable {
        return &self.definition;
    }
}

impl UpnpObject for StateVarInstance {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        return &self.object;
    }

    fn to_xml_element(&self) -> Element {
        self.get_definition().to_xml_element()
    }
}

impl StateVarInstance {
    pub fn new(from: &StateVariable) -> Self {
        Self {
            object: UpnpObjectType {
                name: from.object.name.clone(),
                object_type: "StateVarInstance".to_string(),
            },
            definition: from.clone(),
            value: from.get_default(),
            old_value: from.get_default(),
            last_modified: Utc::now(),
            last_notification: Utc::now()
        }
    }

    pub fn set_value(&mut self, new_value: StateValue) {
        self.old_value = self.value.clone();
        self.value = new_value;
        self.last_modified = Utc::now(); // mise à jour automatique
    }

    /// Accès à la valeur
    pub fn value(&self) -> &StateValue {
        &self.value
    }

    /// Accès au timestamp
    pub fn last_modified(&self) -> DateTime<Utc> {
        self.last_modified
    }

}
