use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::StateVarType;
use once_cell::sync::Lazy;

pub static A_ARG_TYPE_PLAY_SPEED: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    Arc::new(StateVariable::new(StateVarType::String, "A_ARG_TYPE_PlaySpeed".to_string()))
});
