use crate::state_variables::StateVariable;
use crate::variable_types::StateVarType;
use once_cell::sync::Lazy;

pub static SEEKMODE: Lazy<StateVariable> = Lazy::new(|| -> StateVariable {
    StateVariable::new(StateVarType::String, "SeekMode".to_string())
});

