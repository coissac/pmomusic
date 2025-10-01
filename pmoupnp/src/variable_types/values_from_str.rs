use std::convert::TryFrom;
use url::Url;

use crate::variable_types::{StateValue, StateValueError};

impl TryFrom<&str> for StateValue {
    type Error = StateValueError;

    fn try_from(s: &str) -> Result<Self, StateValueError> {
        Ok(StateValue::String(s.to_string()))
    }
}

// Conversion depuis String
impl TryFrom<String> for StateValue {
    type Error = StateValueError;

    fn try_from(s: String) -> Result<Self, StateValueError> {
        Ok(StateValue::String(s))
    }
}