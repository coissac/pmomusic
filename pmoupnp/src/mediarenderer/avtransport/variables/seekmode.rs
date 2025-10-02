use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::StateVarType;
use once_cell::sync::Lazy;

pub static SEEKMODE: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    Arc::new(StateVariable::new(StateVarType::String, "SeekMode".to_string()))
});

