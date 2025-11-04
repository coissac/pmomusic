use std::fmt;
use std::sync::Arc;

use bevy_reflect::Reflect;
use chrono::{DateTime, Utc};
use std::sync::RwLock;
use xmltree::Element;

use crate::{
    UpnpObjectType, UpnpTyped, UpnpTypedInstance,
    object_trait::{UpnpInstance, UpnpObject},
    state_variables::{StateVarInstance, StateVariable, UpnpVariable},
    variable_types::{StateValue, StateValueError, UpnpVarType},
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
            reflexive_cache: RwLock::new(None),
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
            reflexive_cache: RwLock::new(None), // Le cache n'est pas cloné, il sera recalculé si nécessaire
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
                "Value type mismatch".to_string(),
            ));
        }

        // Mise à jour avec les locks
        let mut old_val = self.old_value.write().unwrap();
        let mut val = self.value.write().unwrap();
        let mut modified = self.last_modified.write().unwrap();

        *old_val = val.clone();
        *val = new_value.clone();
        *modified = Utc::now();

        // Invalider le cache réflexif
        {
            let mut cache = self.reflexive_cache.write().unwrap();
            *cache = None;
        }

        // Notifier le service parent si la variable envoie des événements
        if self.is_sending_notification() {
            // Relâcher les locks avant d'appeler le service
            drop(val);
            drop(old_val);
            drop(modified);

            if let Some(weak_service) = self.service.read().unwrap().as_ref() {
                if let Some(service) = weak_service.upgrade() {
                    // Obtenir la valeur réflexive (sans propager l'erreur car on est dans une notification)
                    if let Ok(reflected_value) = self.reflexive_value() {
                        service.event_to_be_sent(self.get_name().to_string(), reflected_value);
                    }
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

    /// Retourne la valeur sous forme réflexive (Reflect).
    ///
    /// Cette méthode utilise un cache pour optimiser les performances lorsqu'un parser
    /// est défini. Si la variable a un parser, la valeur String sera parsée et le résultat
    /// sera mis en cache. Sinon, la StateValue brute est retournée directement comme Reflect.
    ///
    /// Le cache est invalidé automatiquement lors de `set_value()`.
    ///
    /// # Returns
    ///
    /// Un `Arc<dyn Reflect>` contenant soit:
    /// - La valeur parsée (si un parser est défini)
    /// - La StateValue brute (sinon)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let var = StateVarInstance::new(&variable);
    /// let reflected = var.reflexive_value();
    /// // reflected peut maintenant être inspecté avec l'API Reflect
    /// ```
    pub fn reflexive_value(
        &self,
    ) -> Result<Arc<dyn Reflect>, crate::state_variables::StateVariableError> {
        // Vérifier si on a un cache valide
        {
            let cache = self.reflexive_cache.read().unwrap();
            if let Some(cached) = cache.as_ref() {
                return Ok(Arc::clone(cached));
            }
        }

        // Pas de cache, il faut calculer la valeur
        let value = self.value.read().unwrap().clone();

        // Si la variable a un parser, l'utiliser
        if let Some(parser) = &self.model.parse {
            // La valeur doit être une String pour être parsée
            if let crate::variable_types::StateValue::String(s) = &value {
                match parser(s) {
                    Ok(parsed) => {
                        // Convertir Box<dyn Reflect> en Arc<dyn Reflect>
                        let arc_reflect: Arc<dyn Reflect> = Arc::from(parsed);

                        // Mettre en cache
                        let mut cache = self.reflexive_cache.write().unwrap();
                        *cache = Some(Arc::clone(&arc_reflect));

                        return Ok(arc_reflect);
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        // Pas de parser ou la valeur n'est pas une String: convertir la StateValue en Reflect
        let reflected = value.to_reflect();
        let arc_reflect: Arc<dyn Reflect> = Arc::from(reflected);

        // Mettre en cache
        let mut cache = self.reflexive_cache.write().unwrap();
        *cache = Some(Arc::clone(&arc_reflect));

        Ok(arc_reflect)
    }

    /// Convertit la valeur actuelle en Box<dyn Reflect>
    ///
    /// - Si type String ET parser défini : utilise le parser
    /// - Sinon : utilise StateValue::to_reflect() directement
    ///
    /// # Returns
    ///
    /// Un `Box<dyn Reflect>` contenant la valeur actuelle
    pub fn to_reflect(&self) -> Box<dyn Reflect> {
        let current_value = self.value.read().unwrap().clone();
        self.parse_value(current_value)
    }

    pub fn parse_value(&self, value: StateValue) -> Box<dyn Reflect> {
        use crate::variable_types::StateVarType;

        // Parser uniquement pour les String
        if self.as_state_var_type() == StateVarType::String {
            if let StateValue::String(ref s) = value {
                if let Some(ref parser) = self.model.parse {
                    match parser(s) {
                        Ok(reflected) => return reflected,
                        Err(e) => {
                            tracing::warn!(
                                "Failed to parse value '{}' for variable '{}': {:?}, using raw string",
                                s,
                                self.get_name(),
                                e
                            );
                        }
                    }
                }
            }
        }

        value.to_reflect()
    }
    /// Définit la valeur depuis Box<dyn Reflect>
    ///
    /// - Si type String ET marshal défini : utilise le marshal
    /// - Sinon : utilise StateValue::from_reflect() directement
    ///
    /// Puis délègue à set_value() pour la mise à jour et les notifications
    ///
    /// # Arguments
    ///
    /// * `reflect_value` - La nouvelle valeur sous forme Reflect
    ///
    /// # Errors
    ///
    /// Retourne une erreur si :
    /// - La conversion Reflect → StateValue échoue
    /// - Le marshalling échoue
    /// - La mise à jour de la valeur échoue
    pub async fn set_reflect_value(
        &self,
        reflect_value: Box<dyn Reflect>,
    ) -> Result<(), StateValueError> {
        use crate::variable_types::StateVarType;

        let reflect_ref = reflect_value.as_ref();

        // Cas particulier : variable de type String avec marshal
        let state_value = if self.as_state_var_type() == StateVarType::String {
            if let Some(ref marshal) = self.model.marshal {
                match marshal(reflect_ref) {
                    Ok(serialized) => StateValue::String(serialized),
                    Err(e) => {
                        tracing::warn!(
                            "Marshal failed for '{}': {:?}, using default Reflect→StateValue conversion",
                            self.get_name(),
                            e
                        );
                        StateValue::from_reflect(reflect_ref, self.as_state_var_type())?
                    }
                }
            } else {
                StateValue::from_reflect(reflect_ref, self.as_state_var_type())?
            }
        } else {
            StateValue::from_reflect(reflect_ref, self.as_state_var_type())?
        };

        self.set_value(state_value).await
    }
}
