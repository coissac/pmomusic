use std::convert::TryFrom;

use crate::variable_types::{StateValue, StateValueError};


impl TryFrom<&StateValue> for i32 {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            // signés
            StateValue::I1(v) => Ok(*v as i32),
            StateValue::I2(v) => Ok(*v as i32),
            StateValue::I4(v) => Ok(*v),
            StateValue::Int(v) => Ok(*v),

            // non signés
            StateValue::UI1(v) => Ok(*v as i32),
            StateValue::UI2(v) => Ok(*v as i32),
            StateValue::UI4(v) if *v <= i32::MAX as u32 =>Ok(*v as i32),
            StateValue::Boolean(v) => Ok(*v as Self),

            StateValue::String(s) => s.parse::<i32>().map_err(|_| {
                StateValueError::TypeError(format!("Cannot parse '{}' as i32", s))
            }),

            _ => Err(StateValueError::TypeError("Cannot cast to i32".into())),
        }
    }
}

impl TryFrom<StateValue> for i32 {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        i32::try_from(&value)
    }
}

impl From<i32> for StateValue {

    fn from(value: i32) -> Self {
        StateValue::I4(value)
    }
}

