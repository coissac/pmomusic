use std::convert::TryFrom;

use crate::variable_types::{StateValue, StateValueError};


impl TryFrom<&StateValue> for f32 {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        const MAX_EXACT: i32 = 1 << f32::MANTISSA_DIGITS;
        match value {
            // --- Signed integers ---
            StateValue::I1(v) => Ok(*v as f32),
            StateValue::I2(v) => Ok(*v as f32),
            StateValue::I4(v) 
                if *v > -MAX_EXACT &&
                   *v < MAX_EXACT => Ok(*v as f32),
            StateValue::Int(v) 
                if *v >= -MAX_EXACT &&
                   *v <= MAX_EXACT as i32 => Ok(*v as f32),

            // --- Unsigned integers ---
            StateValue::UI1(v) => Ok(*v as f32),
            StateValue::UI2(v) => Ok(*v as f32),
            StateValue::UI4(v) if *v <= MAX_EXACT as u32 => Ok(*v as f32),
            StateValue::UI4(_) => Err(StateValueError::TypeError(
                "Cannot cast UI4 to f32: out of range".into(),
            )),

            // --- Floats ---
            StateValue::R4(v) => Ok(*v), // déjà un f32
            StateValue::R8(v)
                if ! v.is_finite() || 
                (*v <= f32::MAX as f64 && 
                 *v >= f32::MIN as f64) => Ok(*v as f32),
            StateValue::R8(_) => Err(StateValueError::TypeError(
                "Cannot cast R8 to f32: out of range".into(),
            )),
            StateValue::Number(v)
                if ! v.is_finite() || 
                (*v <= f32::MAX as f64 && 
                 *v >= f32::MIN as f64) => Ok(*v as f32),
            StateValue::Number(_) => Err(StateValueError::TypeError(
                "Cannot cast Number to f32: out of range".into(),
            )),
            StateValue::Fixed14_4(v)
                if ! v.is_finite() || 
                (*v <= f32::MAX as f64 && 
                 *v >= f32::MIN as f64) => Ok(*v as f32),
            StateValue::Fixed14_4(_) => Err(StateValueError::TypeError(
                "Cannot cast Fixed14_4 to f32: out of range".into(),
            )),

            StateValue::Boolean(v) => Ok((*v as i32) as Self),

            StateValue::String(s) => s.parse::<f32>().map_err(|_| {
                StateValueError::TypeError(format!("Cannot parse '{}' as f32", s))
            }),

            // --- Par défaut : erreur ---
            _ => Err(StateValueError::TypeError(
                "Cannot cast to f32".into(),
            )),
        }
    }
}

impl TryFrom<StateValue> for f32 {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        f32::try_from(&value)
    }
}

impl From<f32> for StateValue {

    fn from(value: f32) -> Self {
        StateValue::R4(value)
    }
}

