use std::convert::TryFrom;

use crate::variable_types::{StateValue, StateValueError};

// Implémentations TryFrom<StateValue> pour types numériques

impl TryFrom<&StateValue> for i8 {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            StateValue::I1(v) if *v >= 0 => Ok(*v as i8),
            StateValue::I2(v) if *v <= i8::MAX as i16 && *v >= i8::MIN as i16 => Ok(*v as i8),
            StateValue::I4(v) if *v <= i8::MAX as i32 && *v >= i8::MIN as i32 => Ok(*v as i8),
            StateValue::Int(v) if *v <= i8::MAX as i32 && *v >= i8::MIN as i32 => Ok(*v as i8),

            StateValue::UI1(v) if *v <= i8::MAX as u8 => Ok(*v as i8),
            StateValue::UI2(v) if *v <= i8::MAX as u16 => Ok(*v as i8),
            StateValue::UI4(v) if *v <= i8::MAX as u32 => Ok(*v as i8),
            StateValue::Boolean(v) => Ok(*v as Self),

            StateValue::String(s) => s
                .parse::<i8>()
                .map_err(|_| StateValueError::TypeError(format!("Cannot parse '{}' as i8", s))),

            _ => Err(StateValueError::TypeError("Cannot cast to i8".into())),
        }
    }
}

impl TryFrom<StateValue> for i8 {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        i8::try_from(&value)
    }
}

impl From<i8> for StateValue {
    fn from(value: i8) -> Self {
        StateValue::I1(value)
    }
}
