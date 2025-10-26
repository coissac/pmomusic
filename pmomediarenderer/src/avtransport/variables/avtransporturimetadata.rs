use std::sync::Arc;

use bevy_reflect::Reflect;
use htmlescape::decode_html;
use once_cell::sync::Lazy;
use pmodidl::{DIDLLite, MediaMetadataParser};
use pmoupnp::state_variables::{StateVariable, StateVariableError};
use pmoupnp::variable_types::StateVarType;

fn avtransporturimetadataparser(value: &str) -> Result<Box<dyn Reflect>, StateVariableError> {
    // Nettoyage de base
    let trimmed = value.trim();

    // Cas 1 : chaîne vide => pas de métadonnée
    if trimmed.is_empty() {
        return Ok(Box::new(DIDLLite::default()) as Box<dyn Reflect>);
    }

    // Cas 2 : XML échappé (&lt;DIDL-Lite&gt;)
    let decoded = if trimmed.starts_with("&lt;") {
        decode_html(trimmed).unwrap_or_else(|_| trimmed.to_string())
    } else {
        trimmed.to_string()
    };

    // Tentative de parsing
    let didl = DIDLLite::parse(&decoded)
        .map_err(|e| StateVariableError::ParseError(format!("Failed to parse DIDL-Lite: {}", e)))?;

    Ok(Box::new(didl) as Box<dyn Reflect>)
}

fn avtransporturimetadatamarshal(value: &dyn Reflect) -> Result<String, StateVariableError> {
    let didl = value
        .downcast_ref::<DIDLLite>()
        .ok_or_else(|| StateVariableError::ConversionError("DIDLLite".into()))?;
    let xml = quick_xml::se::to_string(didl)
        .map_err(|e| StateVariableError::ConversionError(format!("serialize error: {}", e)))?;
    Ok(xml)
}

pub static AVTRANSPORTURIMETADATA: Lazy<Arc<StateVariable>> =
    Lazy::new(|| -> Arc<StateVariable> {
        let mut sv = StateVariable::new(StateVarType::String, "AVTransportURIMetaData".to_string());

        sv.set_value_parser(Arc::new(avtransporturimetadataparser))
            .expect("Failed to set parser");
        Arc::new(sv)
    });

pub static AVTRANSPORTNEXTURIMETADATA: Lazy<Arc<StateVariable>> =
    Lazy::new(|| -> Arc<StateVariable> {
        let mut sv = StateVariable::new(
            StateVarType::String,
            "AVTransportNextURIMetaData".to_string(),
        );

        sv.set_value_parser(Arc::new(avtransporturimetadataparser))
            .expect("Failed to set parser");

        sv.set_value_marshaler(Arc::new(avtransporturimetadatamarshal))
            .expect("Failed to set mzrshaler");

        Arc::new(sv)
    });
