use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::{StateValue, StateVarType};
use once_cell::sync::Lazy;

pub static PLAYBACKSTORAGEMEDIUM: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let mut sv = StateVariable::new(StateVarType::String, "PlaybackStorageMedium".to_string());

    // Valeurs pour un MediaRenderer audio uniquement (suppression des formats vidéo)
    sv.extend_allowed_values(&[
        StateValue::String("UNKNOWN".to_string()),
        StateValue::String("CD-ROM".to_string()),
        StateValue::String("CD-DA".to_string()),
        StateValue::String("CD-R".to_string()),
        StateValue::String("CD-RW".to_string()),
        StateValue::String("SACD".to_string()),
        StateValue::String("MD-AUDIO".to_string()),
        StateValue::String("DVD-AUDIO".to_string()),
        StateValue::String("DAT".to_string()),
        StateValue::String("HDD".to_string()),
        StateValue::String("NETWORK".to_string()),
        StateValue::String("NONE".to_string()),
        StateValue::String("NOT_IMPLEMENTED".to_string()),
    ]).expect("Cannot set allowed values");

    Arc::new(sv)
});
