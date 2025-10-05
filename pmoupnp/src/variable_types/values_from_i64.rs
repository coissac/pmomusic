use crate::variable_types::{StateValue, StateValueError};
use std::convert::TryFrom;

impl TryFrom<&StateValue> for i64 {
    type Error = StateValueError;

    fn try_from(value: &StateValue) -> Result<Self, Self::Error> {
        match value {
            // signés
            StateValue::I1(v) => Ok(*v as i64),
            StateValue::I2(v) => Ok(*v as i64),
            StateValue::I4(v) => Ok(*v as i64),
            StateValue::Int(v) => Ok(*v as i64),

            // non signés
            StateValue::UI1(v) => Ok(*v as i64),
            StateValue::UI2(v) => Ok(*v as i64),
            StateValue::UI4(v) => Ok(*v as i64), // toujours dans l'intervalle d'un i64

            // booléen
            StateValue::Boolean(v) => Ok(*v as i64),

            // chaîne → i64
            StateValue::String(s) => s
                .parse::<i64>()
                .map_err(|_| StateValueError::TypeError(format!("Cannot parse '{}' as i64", s))),

            _ => Err(StateValueError::TypeError("Cannot cast to i64".into())),
        }
    }
}

impl TryFrom<StateValue> for i64 {
    type Error = StateValueError;

    fn try_from(value: StateValue) -> Result<Self, Self::Error> {
        i64::try_from(&value)
    }
}

impl From<i64> for StateValue {
    fn from(value: i64) -> Self {
        StateValue::Int(value as i32) // ⚠️ choix à discuter : Int est i32, pas i64
    }
}
