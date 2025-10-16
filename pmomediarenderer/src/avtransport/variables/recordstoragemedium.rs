use std::sync::Arc;

use pmoupnp::state_variables::StateVariable;
use pmoupnp::variable_types::StateVarType;
use once_cell::sync::Lazy;

pub static RECORDSTORAGEMEDIUM: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let sv = StateVariable::new(StateVarType::String, "RecordStorageMedium".to_string());
    Arc::new(sv)
});
