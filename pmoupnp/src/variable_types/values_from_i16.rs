use std::convert::TryFrom;

use crate::variable_types::{StateValue, StateValueError};

// Implémentations TryFrom<StateValue> pour types numériques

impl TryFrom<&StateValue> for i16 {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            StateValue::I1(v) => Ok(*v as i16),
            StateValue::I2(v) => Ok(*v),
            StateValue::I4(v) if *v <= i16::MAX as i32 && *v >= i16::MIN as i32 => Ok(*v as i16),
            StateValue::Int(v) if *v <= i16::MAX as i32 && *v >= i16::MIN as i32 => Ok(*v as i16),

            StateValue::UI1(v) => Ok(*v as i16),
            StateValue::UI2(v) if *v <= i16::MAX as u16 => Ok(*v as i16),
            StateValue::UI4(v) if *v <= i16::MAX as u32 => Ok(*v as i16),
            StateValue::Boolean(v) => Ok(*v as Self),

            StateValue::String(s) => s
                .parse::<i16>()
                .map_err(|_| StateValueError::TypeError(format!("Cannot parse '{}' as i16", s))),

            _ => Err(StateValueError::TypeError("Cannot cast to i32".into())),
        }
    }
}

impl TryFrom<StateValue> for i16 {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        i16::try_from(&value)
    }
}

impl From<i16> for StateValue {
    fn from(value: i16) -> Self {
        StateValue::I2(value)
    }
}
