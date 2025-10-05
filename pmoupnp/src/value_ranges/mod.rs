mod methods;

use crate::variable_types::StateValue;

#[derive(Debug, Clone)]
pub struct ValueRange {
    min: StateValue,
    max: StateValue,
}
