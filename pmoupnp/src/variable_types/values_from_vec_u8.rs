use std::convert::TryFrom;
use crate::variable_types::{StateValue, StateValueError};
use base64::{engine::general_purpose::STANDARD, Engine as _};

impl TryFrom<&StateValue> for Vec<u8> {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            // Déjà un vecteur binaire
            StateValue::BinBase64(v) => STANDARD.decode(v).map_err(
                |e| StateValueError::ParseError(format!("Base64 decode error: {}", e))),
            StateValue::BinHex(v) => hex::decode(v).map_err(
                |e| StateValueError::ParseError(format!("BinHex decode error: {}", e))),

            // Conversion depuis une chaîne encodée
            StateValue::String(s) => {
                // Essayer Base64
                if let Ok(bytes) = STANDARD.decode(s) {
                    return Ok(bytes);
                }
                // Essayer Hex
                if let Ok(bytes) = hex::decode(s) {
                    return Ok(bytes);
                }
                Err(StateValueError::ParseError(format!(
                    "Cannot parse string '{}' as binary",
                    s
                )))
            }

            _ => Err(StateValueError::TypeError(
                "Cannot cast to binary Vec<u8>".into(),
            )),
        }
    }
}

impl TryFrom<StateValue> for Vec<u8> {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        Vec::<u8>::try_from(&value)
    }
}
