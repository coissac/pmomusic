use std::fmt;

use chrono::{DateTime, Utc};
use std::sync::RwLock;
use xmltree::Element;

use crate::{
    object_trait::{UpnpInstance, UpnpObject}, 
    state_variables::{StateVarInstance, StateVariable, UpnpVariable}, 
    variable_types::{StateValue, StateValueError, UpnpVarType}, 
    UpnpObjectType, UpnpTyped, UpnpTypedInstance
};

impl UpnpVariable for StateVarInstance {
    fn get_definition(&self) -> &StateVariable {
        return &self.model;
    }
}

impl UpnpObject for StateVarInstance {
    fn to_xml_element(&self) -> Element {
        self.get_definition().to_xml_element()
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
            service: RwLock::new(None),
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
            value: RwLock::new(self.value.read().unwrap().clone()),
            old_value: RwLock::new(self.old_value.read().unwrap().clone()),
            last_modified: RwLock::new(self.last_modified.read().unwrap().clone()),
            last_notification: RwLock::new(self.last_notification.read().unwrap().clone()),
            service: RwLock::new(self.service.read().unwrap().clone()),
        }
    }
}

impl StateVarInstance {
    /// Enregistre le service parent pour cette variable.
    ///
    /// Cette méthode doit être appelée depuis `ServiceInstance::new()` pour
    /// permettre à la variable de notifier le service lorsqu'elle change.
    ///
    /// # Arguments
    ///
    /// * `service` - Arc vers le ServiceInstance parent
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use pmoupnp::services::ServiceInstance;
    /// # use pmoupnp::state_variables::StateVarInstance;
    /// # use std::sync::Arc;
    /// let service_instance = Arc::new(ServiceInstance::new(&service));
    /// let var_instance = Arc::new(StateVarInstance::new(&variable));
    /// var_instance.register_service(Arc::downgrade(&service_instance));
    /// ```
    pub fn register_service(&self, service: std::sync::Weak<crate::services::ServiceInstance>) {
        let mut svc = self.service.write().unwrap();
        *svc = Some(service);
    }

    pub async fn set_value(&self, new_value: StateValue) -> Result<(), StateValueError> {
        // Validation du type
        if self.as_state_var_type() != new_value.as_state_var_type() {
            return Err(StateValueError::TypeError(
                "Value type mismatch".to_string()
            ));
        }

        // Mise à jour avec les locks
        let mut old_val = self.old_value.write().unwrap();
        let mut val = self.value.write().unwrap();
        let mut modified = self.last_modified.write().unwrap();

        *old_val = val.clone();
        *val = new_value.clone();
        *modified = Utc::now();

        // Notifier le service parent si la variable envoie des événements
        if self.is_sending_notification() {
            // Relâcher les locks avant d'appeler le service
            drop(val);
            drop(old_val);
            drop(modified);

            if let Some(weak_service) = self.service.read().unwrap().as_ref() {
                if let Some(service) = weak_service.upgrade() {
                    service.event_to_be_sent(self.get_name().to_string(), new_value.to_string());
                }
            }
        }

        Ok(())
    }
    /// Accès à la valeur
    pub fn value(&self) -> StateValue {
        self.value.read().unwrap().clone()
    }

    /// Accès au timestamp
    pub fn last_modified(&self) -> DateTime<Utc> {
        self.last_modified.read().unwrap().clone()
    }
}
