mod errors;
mod instance_methods;
mod variable_methods;
mod var_set_methods;
mod variable_trait;

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

pub use crate::state_variables::variable_trait::UpnpVariable;
use bevy_reflect::Reflect;
use chrono::{DateTime, Utc};
pub use errors::StateVariableError;

use crate::{
    UpnpObjectType,
    value_ranges::ValueRange,
    variable_types::{StateValue, StateVarType},
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

#[derive(Debug, Default, Clone)]
pub struct StateVariableSet {
    instances: HashMap<String, StateVariable>,
}

pub struct StateVarInstance {
    object: UpnpObjectType,
    definition: StateVariable,
    value: StateValue,
    old_value: StateValue,
    last_modified: DateTime<Utc>,
    last_notification: DateTime<Utc>,
}
