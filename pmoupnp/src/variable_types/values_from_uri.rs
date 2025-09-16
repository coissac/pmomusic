use std::convert::TryFrom;
use url::Url;

use crate::variable_types::{StateValue, StateValueError};

impl TryFrom<StateValue> for Url {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        match value {
            // Si déjà un URI encodé comme StateValue::URI
            StateValue::URI(v) => Ok(v),

            // Si c'est une String, on tente un parse
            StateValue::String(v) => {
                Url::parse(&v)
                    .map_err(|_| StateValueError::TypeError("Invalid URI string".into()))
            }

            // Autres types : erreur
            _ => Err(StateValueError::TypeError("Cannot cast to Url".into())),
        }
    }
}

impl From<Url> for StateValue {

    fn from(value: Url) -> Self {
        StateValue::URI(value)
    }
}
