use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::{StateValue, StateVarType};
use once_cell::sync::Lazy;

pub static TRANSPORTSTATUS: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let mut sv = StateVariable::new(StateVarType::String, "TransportStatus".to_string());

    sv.push_allowed_value(&StateValue::String("OK".to_string()))
        .expect("Cannot add allowed value");
    sv.extend_allowed_values(&[
        StateValue::String("OK".to_string()),
        StateValue::String("ERROR_OCCURRED".to_string()),
    ])
    .expect("Cannt set default value");

    Arc::new(sv)
});
