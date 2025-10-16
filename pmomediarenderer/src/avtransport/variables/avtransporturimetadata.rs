use std::sync::Arc;

use pmoupnp::state_variables::{StateVariable, StateVariableError};
use pmoupnp::variable_types::StateVarType;
use bevy_reflect::Reflect;
use once_cell::sync::Lazy;
use pmodidl::{DIDLLite, MediaMetadataParser};


fn avtransporturimetadataparser(value: &str) -> Result<Box<dyn Reflect>, StateVariableError> {
    // Parse DIDL-Lite
    let didl = DIDLLite::parse(value)
        .map_err(|e| StateVariableError::ParseError(format!("Failed to parse DIDL-Lite: {}", e)))?;
    
    // Retourne le résultat sous forme de Box<dyn Reflect>
    Ok(Box::new(didl) as Box<dyn Reflect>)
}

pub static AVTRANSPORTURIMETADATA: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let mut sv = StateVariable::new(StateVarType::String, "AVTransportURIMetaData".to_string());

    sv.set_value_parser(Arc::new(avtransporturimetadataparser)).expect("Failed to set parser");
    Arc::new(sv)
});

pub static AVTRANSPORTNEXTURIMETADATA: Lazy<Arc<StateVariable>> = Lazy::new(|| -> Arc<StateVariable> {
    let mut sv = StateVariable::new(StateVarType::String, "AVTransportNextURIMetaData".to_string());

    sv.set_value_parser(Arc::new(avtransporturimetadataparser)).expect("Failed to set parser");
    Arc::new(sv)
});
