use std::convert::TryFrom;
use uuid::Uuid;

use crate::variable_types::{StateValue, StateValueError};

impl TryFrom<StateValue> for Uuid {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        match value {
            // Si déjà un URI encodé comme StateValue::URI
            StateValue::UUID(v) => Ok(v),

            // Si c'est une String, on tente un parse
            StateValue::String(v) => {
                Uuid::parse_str(&v)
                    .map_err(|_| StateValueError::TypeError("Invalid UUID string".into()))
            }

            // Autres types : erreur
            _ => Err(StateValueError::TypeError("Cannot cast to Uuid".into())),
        }
    }
}

impl From<Uuid> for StateValue {

    fn from(value: Uuid) -> Self {
        StateValue::UUID(value)
    }
}
