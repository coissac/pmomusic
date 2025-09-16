use std::convert::TryFrom;


use crate::variable_types::{StateValue, StateValueError};

impl TryFrom<&StateValue> for u32 {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            StateValue::UI1(v) => Ok(*v as Self),
            StateValue::UI2(v) => Ok(*v as Self),
            StateValue::UI4(v)  => Ok(*v),
            StateValue::I1(v) if *v >= 0 => Ok(*v as Self),
            StateValue::I2(v) if *v >= 0 => Ok(*v as Self),
            StateValue::I4(v) if *v >= 0 => Ok(*v as Self),
            StateValue::Int(v) if *v >= 0 => Ok(*v as Self),
            StateValue::Boolean(v) => Ok(*v as Self),

            StateValue::String(s) => s.parse::<u32>().map_err(|_| {
                StateValueError::TypeError(format!("Cannot parse '{}' as u32", s))
            }),

            _ => Err(StateValueError::TypeError("Cannot cast to u32".into())),
        }
    }
}

impl TryFrom<StateValue> for u32 {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        u32::try_from(&value)
    }
}



impl From<u32> for StateValue {

    fn from(value: u32) -> Self {
        StateValue::UI4(value)
    }
}

