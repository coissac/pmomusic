use std::sync::Arc;

use crate::state_variables::StateVariable;
use crate::variable_types::{StateValue, StateVarType};
use once_cell::sync::Lazy;

pub static TRANSPORTSTATE: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let mut sv = StateVariable::new(StateVarType::String, "TransportState".to_string());

    sv.extend_allowed_values(&[ 
        StateValue::String("STOPPED".to_string()),
		StateValue::String("PLAYING".to_string()),
		StateValue::String("RECORDING".to_string()),
		StateValue::String("TRANSITIONING".to_string()),
		StateValue::String("PAUSED_PLAYBACK".to_string()),
		StateValue::String("PAUSED_RECORDING".to_string()),
		StateValue::String("NO_MEDIA_PRESENT".to_string()), 
        ]).expect("Cannt set default value");
        
    sv.set_send_notification();

    Arc::new(sv)
});

