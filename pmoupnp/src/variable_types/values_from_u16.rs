use std::convert::TryFrom;

use crate::variable_types::{StateValue, StateValueError};

// Implémentations TryFrom<StateValue> pour types numériques

impl TryFrom<&StateValue> for u16 {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            StateValue::UI1(v) => Ok(*v as Self),
            StateValue::UI2(v) => Ok(*v),
            StateValue::UI4(v) if *v <= i16::MAX as u32 => Ok(*v as Self),
            StateValue::I1(v) if *v >= 0 => Ok(*v as Self),
            StateValue::I2(v) if *v >= 0 => Ok(*v as Self),
            StateValue::I4(v) if *v >= 0 && *v <= u16::MAX as i32 => Ok(*v as Self),
            StateValue::Int(v) if *v >= 0 && *v <= u16::MAX as i32 => Ok(*v as Self),
            StateValue::Boolean(v) => Ok(*v as Self),

            StateValue::String(s) => s
                .parse::<u16>()
                .map_err(|_| StateValueError::TypeError(format!("Cannot parse '{}' as u16", s))),

            _ => Err(StateValueError::TypeError("Cannot cast to u16".into())),
        }
    }
}

impl TryFrom<StateValue> for u16 {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        u16::try_from(&value)
    }
}

impl From<u16> for StateValue {
    fn from(value: u16) -> Self {
        StateValue::UI2(value)
    }
}
