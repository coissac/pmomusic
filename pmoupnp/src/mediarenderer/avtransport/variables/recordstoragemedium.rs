use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::StateVarType;
use once_cell::sync::Lazy;

pub static RECORDSTORAGEMEDIUM: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let sv = StateVariable::new(StateVarType::String, "RecordStorageMedium".to_string());
    Arc::new(sv)
});
