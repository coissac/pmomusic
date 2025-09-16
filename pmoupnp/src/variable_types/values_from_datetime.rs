use std::convert::TryFrom;
use chrono::{DateTime,FixedOffset};
use crate::variable_types::{StateValue, StateValueError};

impl TryFrom<&StateValue> for DateTime<FixedOffset> {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            StateValue::DateTimeTZ(v) => Ok(v.clone()),
            StateValue::TimeTZ(v) => Ok(v.clone()),
            StateValue::String(v) => DateTime::parse_from_rfc3339(v)
                .map_err(|e| StateValueError::ParseError(format!("Cannot parse DateTimeTZ from string '{}': {}", v, e))),
            _ => Err(StateValueError::TypeError("Cannot cast to DateTime<FixedOffset>".into())),
        }
    }
}

impl TryFrom<StateValue> for DateTime<FixedOffset> {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        DateTime::<FixedOffset>::try_from(&value)
    }
}

impl From<DateTime<FixedOffset>> for StateValue {

    fn from(value: DateTime<FixedOffset>) -> Self {
        StateValue::DateTimeTZ(value)
    }
}

