use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::{StateValue, StateVarType};
use once_cell::sync::Lazy;

pub static A_ARG_TYPE_SEEKMODE: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let mut sv = StateVariable::new(StateVarType::UI4, "A_ARG_TYPE_SeekMode".to_string());

    sv.extend_allowed_values(&[ 
        StateValue::String("TRACK_NR".to_string()),
		StateValue::String("REL_TIME".to_string()),
		StateValue::String("ABS_TIME".to_string()),
        ]).expect("Cannot set default value");

    Arc::new(sv)
});
