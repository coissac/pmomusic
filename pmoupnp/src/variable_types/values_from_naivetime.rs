use std::convert::TryFrom;
use chrono::NaiveTime;
use crate::variable_types::{StateValue, StateValueError};

impl TryFrom<&StateValue> for NaiveTime {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            StateValue::Time(v) => Ok(v.clone()),
            StateValue::String(v) => NaiveTime::parse_from_str(&v, "%H:%M:%S")
                .map_err(|e| StateValueError::ParseError(format!("Cannot parse Time from string '{}': {}", v, e))),
            _ => Err(StateValueError::TypeError("Cannot cast to NaiveTime".into())),
        }
    }
}

impl TryFrom<StateValue> for NaiveTime {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        NaiveTime::try_from(&value)
    }
}

impl From<NaiveTime> for StateValue {

    fn from(value: NaiveTime) -> Self {
        StateValue::Time(value)
    }
}
