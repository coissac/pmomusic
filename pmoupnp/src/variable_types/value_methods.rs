use std::cmp::Ordering;

use crate::variable_types::{StateValue, StateValueError, StateVarType, type_trait::UpnpVarType};

impl UpnpVarType for StateValue {
    fn as_state_var_type(&self) -> StateVarType {
        StateVarType::from(self) // utilise ton From<&StateValue> existant
    }
}

impl PartialEq for StateValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (a, b) if a.is_integer() && b.is_integer() => {
                if let (Ok(ia), Ok(ib)) = (i64::try_from(a), i64::try_from(b)) {
                    return ia == ib;
                };
                return false;
            }
            (a, b) if a.is_float() && b.is_float() => {
                if let (Ok(a), Ok(b)) = (f64::try_from(self), f64::try_from(other)) {
                    return a == b; // NaN respecte la sémantique IEEE (NaN != NaN)
                }
                return false;
            }
            (a, b) if a.is_string() && b.is_string() => {
                let (a, b) = (self.to_string(), other.to_string());
                return a == b; // NaN respecte la sémantique IEEE (NaN != NaN)
            }
            (StateValue::Date(a), StateValue::Date(b)) => {
                return a == b;
            }
            (StateValue::Time(a), StateValue::Time(b)) => {
                return a == b;
            }
            (StateValue::DateTime(a), StateValue::DateTime(b)) => {
                return a == b;
            }
            (StateValue::DateTimeTZ(a), StateValue::DateTimeTZ(b)) => {
                return a == b;
            }
            (StateValue::TimeTZ(a), StateValue::TimeTZ(b)) => {
                return a == b;
            }

            (_, _) => return false,
        }
    }
}

impl PartialOrd for StateValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (a, b) if a.is_integer() && b.is_integer() => {
                if let (Ok(ia), Ok(ib)) = (i64::try_from(a), i64::try_from(b)) {
                    return Some(ia.cmp(&ib));
                };
                return None;
            }
            (a, b) if a.is_float() && b.is_float() => {
                if let (Ok(a), Ok(b)) = (f64::try_from(self), f64::try_from(other)) {
                    return a.partial_cmp(&b); // NaN respecte la sémantique IEEE (NaN != NaN)
                }
                return None;
            }
            (a, b) if a.is_string() && b.is_string() => {
                let (a, b) = (self.to_string(), other.to_string());
                return Some(a.cmp(&b)); // NaN respecte la sémantique IEEE (NaN != NaN)
            }
            (StateValue::Date(a), StateValue::Date(b)) => {
                return Some(a.cmp(&b));
            }
            (StateValue::Time(a), StateValue::Time(b)) => {
                return Some(a.cmp(&b));
            }
            (StateValue::DateTime(a), StateValue::DateTime(b)) => {
                return Some(a.cmp(&b));
            }
            (StateValue::DateTimeTZ(a), StateValue::DateTimeTZ(b)) => {
                return Some(a.cmp(&b));
            }
            (StateValue::TimeTZ(a), StateValue::TimeTZ(b)) => {
                return Some(a.cmp(&b));
            }
            (_, _) => return None,
        }
    }
}

impl StateValue {
    /// Parse une chaîne de caractères en StateValue selon le type spécifié.
    ///
    /// # Arguments
    ///
    /// * `s` - La chaîne à parser
    /// * `var_type` - Le type de variable attendu
    ///
    /// # Returns
    ///
    /// `Ok(StateValue)` si le parsing réussit, `Err(StateValueError)` sinon.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use pmoupnp::variable_types::{StateValue, StateVarType};
    ///
    /// let value = StateValue::from_string("42", &StateVarType::UI4).unwrap();
    /// assert_eq!(value, StateValue::UI4(42));
    ///
    /// let value = StateValue::from_string("true", &StateVarType::Boolean).unwrap();
    /// assert_eq!(value, StateValue::Boolean(true));
    /// ```
    pub fn from_string(s: &str, var_type: &StateVarType) -> Result<Self, StateValueError> {
        use chrono::NaiveDate;
        use url::Url;
        use uuid::Uuid;

        match var_type {
            StateVarType::UI1 => s.parse::<u8>()
                .map(StateValue::UI1)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse UI1: {}", e))),
            StateVarType::UI2 => s.parse::<u16>()
                .map(StateValue::UI2)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse UI2: {}", e))),
            StateVarType::UI4 => s.parse::<u32>()
                .map(StateValue::UI4)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse UI4: {}", e))),
            StateVarType::I1 => s.parse::<i8>()
                .map(StateValue::I1)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse I1: {}", e))),
            StateVarType::I2 => s.parse::<i16>()
                .map(StateValue::I2)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse I2: {}", e))),
            StateVarType::I4 | StateVarType::Int => s.parse::<i32>()
                .map(StateValue::I4)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse I4/Int: {}", e))),
            StateVarType::R4 => s.parse::<f32>()
                .map(StateValue::R4)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse R4: {}", e))),
            StateVarType::R8 | StateVarType::Number | StateVarType::Fixed14_4 => s.parse::<f64>()
                .map(StateValue::R8)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse R8/Number: {}", e))),
            StateVarType::Char => s.chars().next()
                .ok_or_else(|| StateValueError::ParseError("Empty string for Char".to_string()))
                .map(StateValue::Char),
            StateVarType::String => Ok(StateValue::String(s.to_string())),
            StateVarType::Boolean => {
                match s.to_lowercase().as_str() {
                    "true" | "1" | "yes" => Ok(StateValue::Boolean(true)),
                    "false" | "0" | "no" => Ok(StateValue::Boolean(false)),
                    _ => Err(StateValueError::ParseError(format!("Invalid boolean value: {}", s))),
                }
            }
            StateVarType::BinBase64 => Ok(StateValue::BinBase64(s.to_string())),
            StateVarType::BinHex => Ok(StateValue::BinHex(s.to_string())),
            StateVarType::Date => NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map(StateValue::Date)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse Date: {}", e))),
            StateVarType::DateTime => chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
                .map(StateValue::DateTime)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse DateTime: {}", e))),
            StateVarType::DateTimeTZ => chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| StateValue::DateTimeTZ(dt.into()))
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse DateTimeTZ: {}", e))),
            StateVarType::Time => chrono::NaiveTime::parse_from_str(s, "%H:%M:%S")
                .map(StateValue::Time)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse Time: {}", e))),
            StateVarType::TimeTZ => chrono::DateTime::parse_from_rfc3339(&format!("1970-01-01T{}", s))
                .map(|dt| StateValue::TimeTZ(dt.into()))
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse TimeTZ: {}", e))),
            StateVarType::UUID => Uuid::parse_str(s)
                .map(StateValue::UUID)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse UUID: {}", e))),
            StateVarType::URI => Url::parse(s)
                .map(StateValue::URI)
                .map_err(|e| StateValueError::ParseError(format!("Failed to parse URI: {}", e))),
        }
    }
}
