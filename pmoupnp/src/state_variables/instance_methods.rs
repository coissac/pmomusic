use std::fmt;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use xmltree::Element;

use crate::{
    object_trait::{UpnpInstance, UpnpObject}, state_variables::{StateVarInstance, StateVariable, UpnpVariable}, variable_types::{StateValue, StateValueError, UpnpVarType}, UpnpObjectType, UpnpTyped, UpnpTypedInstance, UpnpTypedObject
};

impl UpnpVariable for StateVarInstance {
    fn get_definition(&self) -> &StateVariable {
        return &self.model;
    }
}

impl UpnpObject for StateVarInstance {
    async fn to_xml_element(&self) -> Element {
        self.get_definition().to_xml_element().await
    }
}

impl UpnpVarType for StateVarInstance {
    fn as_state_var_type(&self) -> crate::variable_types::StateVarType {
        self.get_definition().as_state_var_type()
    }
}

impl UpnpInstance for StateVarInstance {
    type Model = StateVariable;

    fn new(from: &StateVariable) -> Self {
        Self {
            object: UpnpObjectType {
                name: from.object.name.clone(),
                object_type: "StateVarInstance".to_string(),
            },
            model: from.clone(),
            value: RwLock::new(from.get_default()),
            old_value: RwLock::new(from.get_default()),
            last_modified: RwLock::new(Utc::now()),
            last_notification: RwLock::new(Utc::now()),
        }
    }

}

impl UpnpTyped for StateVarInstance {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        return &self.object;
    }
}

impl UpnpTypedInstance for StateVarInstance {

    fn get_model(&self) -> &Self::Model {
        &self.model
    }
}

impl fmt::Debug for StateVarInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateVarInstance")
            .field("object", &self.object)
            .field("model", &self.model)
            .field("value", &self.value)
            .field("old_value", &self.old_value)
            .field("last_modified", &self.last_modified)
            .field("last_notification", &self.last_notification)
            .finish()
    }
}

impl Clone for StateVarInstance {
    fn clone(&self) -> Self {
        Self {
            object: self.object.clone(),
            model: self.model.clone(),
            value: RwLock::new(self.value.blocking_read().clone()),
            old_value: RwLock::new(self.old_value.blocking_read().clone()),
            last_modified: RwLock::new(self.last_modified.blocking_read().clone()),
            last_notification: RwLock::new(self.last_notification.blocking_read().clone()),
        }
    }
}

impl StateVarInstance {
    pub async fn set_value(&self, new_value: StateValue) -> Result<(), StateValueError> {
        // Validation du type
        if self.as_state_var_type() != new_value.as_state_var_type() {
            return Err(StateValueError::TypeError(
                "Value type mismatch".to_string()
            ));
        }
        
        // Mise à jour avec les locks
        let mut old_val = self.old_value.write().await;
        let mut val = self.value.write().await;
        let mut modified = self.last_modified.write().await;
        
        *old_val = val.clone();
        *val = new_value;
        *modified = Utc::now();
        
        Ok(())
    }
    /// Accès à la valeur
    pub fn value(&self) -> StateValue {
        self.value.blocking_read().clone()
    }

    /// Accès au timestamp
    pub fn last_modified(&self) -> DateTime<Utc> {
        self.last_modified.blocking_read().clone()
    }
}
