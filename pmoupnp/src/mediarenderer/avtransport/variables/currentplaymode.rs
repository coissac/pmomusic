use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::{StateValue, StateVarType};
use once_cell::sync::Lazy;

pub static CURRENTPLAYMODE: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let mut sv = StateVariable::new(StateVarType::String, "CurrentPlayMode".to_string());

    sv.extend_allowed_values(&[
        StateValue::String("NORMAL".to_string()),
        StateValue::String("SHUFFLE".to_string()),
        StateValue::String("REPEAT_ONE".to_string()),
        StateValue::String("REPEAT_ALL".to_string()),
        StateValue::String("RANDOM".to_string()),
        StateValue::String("DIRECT_1".to_string()),
        StateValue::String("INTRO".to_string()),
    ]).expect("Cannot set allowed values");

    sv.set_default(&StateValue::String("NORMAL".to_string()))
        .expect("Cannot set default value");

    Arc::new(sv)
});
