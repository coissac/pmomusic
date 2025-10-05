use std::cmp::Ordering;

use crate::variable_types::{StateValue, StateVarType, type_trait::UpnpVarType};


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
