use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::StateVarType;
use once_cell::sync::Lazy;

pub static AVTRANSPORTURI: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    Arc::new(StateVariable::new(StateVarType::String, "AVTransportURI".to_string()))
});

pub static AVTRANSPORTNEXTURI: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    Arc::new(StateVariable::new(StateVarType::String, "AVTransportNextURI".to_string()))
});
