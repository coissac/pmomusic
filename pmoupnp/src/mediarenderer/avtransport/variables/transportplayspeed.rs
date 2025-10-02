use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::{StateValue, StateVarType};
use once_cell::sync::Lazy;

pub static TRANSPORTPLAYSPEED: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let mut sv = StateVariable::new(StateVarType::String, "TransportPlaySpeed".to_string());

    sv.push_allowed_value(&StateValue::String("1".to_string())).expect("Cannot add allowed value");
    sv.set_default(&StateValue::String("1".to_string())).expect("Cannt set default value");

    Arc::new(sv)
});

