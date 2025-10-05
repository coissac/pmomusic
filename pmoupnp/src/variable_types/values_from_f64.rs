use std::convert::TryFrom;

use crate::variable_types::{StateValue, StateValueError};

impl TryFrom<&StateValue> for f64 {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            // --- Signed integers ---
            StateValue::I1(v) => Ok(*v as f64),
            StateValue::I2(v) => Ok(*v as f64),
            StateValue::I4(v) => Ok(*v as f64),
            StateValue::Int(v) => Ok(*v as f64),

            // --- Unsigned integers ---
            StateValue::UI1(v) => Ok(*v as f64),
            StateValue::UI2(v) => Ok(*v as f64),
            StateValue::UI4(v) => Ok(*v as f64),

            // --- Floats ---
            StateValue::R4(v) => Ok(*v as f64),
            StateValue::R8(v) => Ok(*v),
            StateValue::Number(v) => Ok(*v),
            StateValue::Fixed14_4(v) => Ok(*v),

            StateValue::Boolean(v) => Ok((*v as i32) as Self),

            StateValue::String(s) => s
                .parse::<f64>()
                .map_err(|_| StateValueError::TypeError(format!("Cannot parse '{}' as f64", s))),

            // --- Par défaut : erreur ---
            _ => Err(StateValueError::TypeError("Cannot cast to f64".into())),
        }
    }
}

impl TryFrom<StateValue> for f64 {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        f64::try_from(&value)
    }
}

impl From<f64> for StateValue {
    fn from(value: f64) -> Self {
        StateValue::R8(value)
    }
}
