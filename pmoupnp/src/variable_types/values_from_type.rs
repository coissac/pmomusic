use crate::variable_types::{StateValue, StateVarType};

impl From<&StateValue> for StateVarType {
    fn from(value: &StateValue) -> Self {
        match value {
            StateValue::UI1(_) => StateVarType::UI1,
            StateValue::UI2(_) => StateVarType::UI2,
            StateValue::UI4(_) => StateVarType::UI4,
            StateValue::I1(_) => StateVarType::I1,
            StateValue::I2(_) => StateVarType::I2,
            StateValue::I4(_) => StateVarType::I4,
            StateValue::Int(_) => StateVarType::Int,
            StateValue::R4(_) => StateVarType::R4,
            StateValue::R8(_) => StateVarType::R8,
            StateValue::Number(_) => StateVarType::Number,
            StateValue::Fixed14_4(_) => StateVarType::Fixed14_4,
            StateValue::Char(_) => StateVarType::Char,
            StateValue::String(_) => StateVarType::String,
            StateValue::BinBase64(_) => StateVarType::BinBase64,
            StateValue::BinHex(_) => StateVarType::BinHex,
            StateValue::URI(_) => StateVarType::URI,
            StateValue::UUID(_) => StateVarType::UUID,
            StateValue::Date(_) => StateVarType::Date,
            StateValue::DateTime(_) => StateVarType::DateTime,
            StateValue::DateTimeTZ(_) => StateVarType::DateTimeTZ,
            StateValue::Time(_) => StateVarType::Time,
            StateValue::TimeTZ(_) => StateVarType::TimeTZ,
            StateValue::Boolean(_) => StateVarType::Boolean,
        }
    }
}

impl From<StateValue> for StateVarType {
    fn from(value: StateValue) -> Self {
        StateVarType::from(&value)
    }
}
