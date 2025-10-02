use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::StateVarType;
use once_cell::sync::Lazy;

pub static A_ARG_TYPE_INSTANCE_ID: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    Arc::new(StateVariable::new(StateVarType::UI4, "A_ARG_TYPE_InstanceID".to_string()))
});
