use crate::state_variables::StateVariable;
use crate::variable_types::StateVarType;
use once_cell::sync::Lazy;

pub static AVTRANSPORTURI: Lazy<StateVariable> = Lazy::new(|| -> StateVariable {
    StateVariable::new(StateVarType::String, "AVTransportURI".to_string())
});
