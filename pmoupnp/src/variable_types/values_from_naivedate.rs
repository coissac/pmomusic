use crate::variable_types::{StateValue, StateValueError};
use chrono::NaiveDate;
use std::convert::TryFrom;

impl TryFrom<&StateValue> for NaiveDate {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            StateValue::Date(v) => Ok(v.clone()),
            StateValue::String(v) => NaiveDate::parse_from_str(v, "%Y-%m-%d").map_err(|e| {
                StateValueError::ParseError(format!("Cannot parse Date from string '{}': {}", v, e))
            }),
            _ => Err(StateValueError::TypeError(
                "Cannot cast to NaiveDate".into(),
            )),
        }
    }
}

impl TryFrom<StateValue> for NaiveDate {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        NaiveDate::try_from(&value)
    }
}

impl From<NaiveDate> for StateValue {
    fn from(value: NaiveDate) -> Self {
        StateValue::Date(value)
    }
}
