use std::sync::Arc;

use once_cell::sync::Lazy;
use pmoupnp::state_variables::StateVariable;
use pmoupnp::variable_types::StateVarType;

pub static RECORDSTORAGEMEDIUM: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let sv = StateVariable::new(StateVarType::String, "RecordStorageMedium".to_string());
    Arc::new(sv)
});
