use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::StateVarType;
use once_cell::sync::Lazy;

pub static POSSIBLERECORDSTORAGEMEDIA: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let sv = StateVariable::new(StateVarType::String, "PossibleRecordStorageMedia".to_string());
    Arc::new(sv)
});
