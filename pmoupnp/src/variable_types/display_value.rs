use base64::Engine;
use base64::engine::general_purpose;
use std::fmt;

use crate::variable_types::StateValue;

impl fmt::Display for StateValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Numériques
            StateValue::UI1(v) => write!(f, "{}", v),
            StateValue::UI2(v) => write!(f, "{}", v),
            StateValue::UI4(v) => write!(f, "{}", v),
            StateValue::I1(v) => write!(f, "{}", v),
            StateValue::I2(v) => write!(f, "{}", v),
            StateValue::I4(v) => write!(f, "{}", v),
            StateValue::Int(v) => write!(f, "{}", v),
            StateValue::R4(v) => write!(f, "{}", v),
            StateValue::R8(v) => write!(f, "{}", v),
            StateValue::Number(v) => write!(f, "{}", v),
            StateValue::Fixed14_4(v) => write!(f, "{}", v),

            // Types déjà Display
            StateValue::Char(v) => write!(f, "{}", v),
            StateValue::String(v) => write!(f, "{}", v),
            StateValue::UUID(v) => write!(f, "{}", v),
            StateValue::URI(v) => write!(f, "{}", v),

            // Booléen : 1 ou 0
            StateValue::Boolean(v) => write!(f, "{}", if *v { "1" } else { "0" }),

            // Encodages binaires
            StateValue::BinBase64(v) => write!(f, "{}", general_purpose::URL_SAFE.encode(v)),
            StateValue::BinHex(v) => write!(f, "{}", hex::encode(v)),

            // Dates et temps
            StateValue::Date(v) => write!(f, "{}", v.format("%Y-%m-%d")),
            StateValue::DateTime(v) => write!(f, "{}", v.format("%Y-%m-%dT%H:%M:%S")),
            StateValue::DateTimeTZ(v) => write!(f, "{}", v.to_rfc3339()),
            StateValue::Time(v) => write!(f, "{}", v.format("%H:%M:%S")),
            StateValue::TimeTZ(v) => write!(f, "{}", v.format("%H:%M:%S%z")),
        }
    }
}
