use std::convert::TryFrom;
use chrono::NaiveDateTime;
use crate::variable_types::{StateValue, StateValueError};

impl TryFrom<&StateValue> for NaiveDateTime {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            StateValue::DateTime(v) => Ok(v.clone()),
            StateValue::String(v) => NaiveDateTime::parse_from_str(&v, "%Y-%m-%dT%H:%M:%S")
                .map_err(|e| StateValueError::ParseError(format!("Cannot parse DateTime from string '{}': {}", v, e))),
            _ => Err(StateValueError::TypeError("Cannot cast to NaiveDateTime".into())),
        }
    }
}

impl TryFrom<StateValue> for NaiveDateTime {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        NaiveDateTime::try_from(&value)
    }
}

impl From<NaiveDateTime> for StateValue {

    fn from(value: NaiveDateTime) -> Self {
        StateValue::DateTime(value)
    }
}
