use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::StateVarType;
use once_cell::sync::Lazy;

pub static CURRENTTRACKDURATION: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let mut sv = StateVariable::new(StateVarType::String, "CurrentTrackDuration".to_string());

    sv.set_send_notification();

    Arc::new(sv)
});

pub static ABSOLUTETIMEPOSITION: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let mut sv = StateVariable::new(StateVarType::String, "AbsoluteTimePosition".to_string());

    sv.set_send_notification();

    Arc::new(sv)
});

pub static RELATIVETIMEPOSITION: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let mut sv = StateVariable::new(StateVarType::String, "RelativeTimePosition".to_string());

    sv.set_send_notification();

    Arc::new(sv)
});

