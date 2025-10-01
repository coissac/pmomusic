use std::convert::TryFrom;

use crate::variable_types::{StateValue, StateValueError};

// Implémentations TryFrom<StateValue> pour types numériques

impl TryFrom<&StateValue> for u8 {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            StateValue::UI1(v) => Ok(*v),
            StateValue::UI2(v) if *v <= u8::MAX as u16 => Ok(*v as u8),
            StateValue::UI4(v) if *v <= i8::MAX as u32 => Ok(*v as u8),
            StateValue::I1(v) if *v >= 0 => Ok(*v as u8),
            StateValue::I2(v) if *v >= 0 && *v <= u8::MAX as i16 => Ok(*v as u8),
            StateValue::I4(v) if *v >= 0 && *v <= u8::MAX as i32 => Ok(*v as u8),
            StateValue::Int(v) if *v >= 0 && *v <= u8::MAX as i32 => Ok(*v as u8),
            StateValue::Boolean(v) => Ok(*v as Self),

            StateValue::String(s) => s
                .parse::<u8>()
                .map_err(|_| StateValueError::TypeError(format!("Cannot parse '{}' as u8", s))),

            _ => Err(StateValueError::TypeError("Cannot cast to u8".into())),
        }
    }
}

impl TryFrom<StateValue> for u8 {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        u8::try_from(&value)
    }
}

impl From<u8> for StateValue {
    fn from(value: u8) -> Self {
        StateValue::UI1(value)
    }
}
