use std::cmp::Ordering;

use crate::{
    value_ranges::ValueRange,
    variable_types::{StateValue, StateValueError, StateVarType, UpnpVarType},
};

impl UpnpVarType for ValueRange {
    fn as_state_var_type(&self) -> StateVarType {
        self.min.as_state_var_type() // utilise ton From<&StateValue> existant
    }
}

impl ValueRange {
    pub fn new(min: &StateValue, max: &StateValue) -> Result<Self, StateValueError> {
        if min.as_state_var_type() != max.as_state_var_type() {
            return Err(StateValueError::TypeError(
                "min and max do not belong the same time".to_string(),
            ));
        }

        // Vérifier que min <= max
        if let Some(cmp) = min.partial_cmp(max) {
            if cmp == Ordering::Greater {
                return Err(StateValueError::RangeError(
                    "Minimum cannot be greater than maximum".to_string(),
                ));
            }
        }

        Ok(Self {
            min: min.clone(),
            max: max.clone(),
        })
    }

    pub fn get_minimum(self: &ValueRange) -> StateValue {
        return self.min.clone();
    }

    pub fn set_minimum(&mut self, value: &StateValue) {
        self.min = value.clone()
    }

    pub fn get_maximum(self: &ValueRange) -> StateValue {
        return self.max.clone();
    }

    pub fn set_maximum(&mut self, value: &StateValue) {
        self.max = value.clone()
    }

    pub fn is_in_range(&self, value: &StateValue) -> bool {
        if self.as_state_var_type() == value.as_state_var_type()
            && let Some(cmp) = self.min.partial_cmp(value)
        {
            if cmp == Ordering::Greater {
                return false;
            }
            if let Some(cmp2) = self.max.partial_cmp(value) {
                if cmp2 == Ordering::Less {
                    return false;
                }
                return true;
            }
        }
        return false;
    }
}
