mod errors;
mod instance_methods;
mod macros;
mod variable_methods;
mod var_set_methods;
mod var_inst_set_methods;
mod variable_trait;

use std::{
    collections::HashMap,
    sync::Arc,
};

pub use crate::state_variables::variable_trait::UpnpVariable;
use bevy_reflect::Reflect;
use chrono::{DateTime, Utc};
pub use errors::StateVariableError;
use std::sync::RwLock;

use crate::{
    value_ranges::ValueRange, 
    variable_types::{StateValue, StateVarType}, 
    UpnpObjectSet, UpnpObjectType,
};

/// Type pour les fonctions de condition d'événement
pub type StateConditionFunc = Arc<dyn Fn(&StateVarInstance) -> bool + Send + Sync>;

/// Type pour les fonctions de parsing de valeurs depuis des chaînes
pub type StringValueParser =
    Arc<dyn Fn(&str) -> Result<Box<dyn Reflect>, StateVariableError> + Send + Sync>;

/// Type pour les fonctions de sérialisation de valeurs vers des chaînes
pub type ValueSerializer =
    Arc<dyn Fn(&StateValue) -> Result<String, StateVariableError> + Send + Sync>;

pub struct StateVariable {
    object: UpnpObjectType,
    value_type: StateVarType,
    step: Option<StateValue>,
    modifiable: bool,
    event_conditions: Arc<RwLock<HashMap<String, StateConditionFunc>>>,
    description: String,
    default_value: Option<StateValue>,
    value_range: Option<ValueRange>,
    allowed_values: Arc<RwLock<Vec<StateValue>>>,
    send_events: bool,
    parse: Option<StringValueParser>,
    marshal: Option<ValueSerializer>,
}

pub type StateVariableSet = UpnpObjectSet<StateVariable>;

pub struct StateVarInstance {
    object: UpnpObjectType,
    model: StateVariable,
    value: RwLock<StateValue>,
    old_value: RwLock<StateValue>,
    last_modified: RwLock<DateTime<Utc>>,
    last_notification: RwLock<DateTime<Utc>>,
    /// Pointeur vers le service parent (interior mutability)
    service: RwLock<Option<std::sync::Weak<crate::services::ServiceInstance>>>,
    /// Cache pour la valeur réflexive (utilisé quand un parser est défini)
    reflexive_cache: RwLock<Option<Arc<dyn Reflect>>>,
}

pub type StateVarInstanceSet = UpnpObjectSet<StateVarInstance>;

