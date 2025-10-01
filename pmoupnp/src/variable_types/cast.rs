use crate::variable_types::{StateValue, StateValueError, StateVarType};
use std::convert::TryFrom;

impl StateValue {
    pub fn try_cast(&self, target: StateVarType) -> Result<StateValue, StateValueError> {
        let source = StateVarType::from(self);

        // Identité (même type)
        if source == target {
            return Ok(self.clone());
        }

        match (self, target) {
            (val, StateVarType::String) => Ok(StateValue::String(val.to_string())),

            (_, StateVarType::UI1) => Ok(StateValue::UI1(u8::try_from(self)?)),
            (_, StateVarType::UI2) => Ok(StateValue::UI2(u16::try_from(self)?)),
            (_, StateVarType::UI4) => Ok(StateValue::UI4(u32::try_from(self)?)),
            (_, StateVarType::I1) => Ok(StateValue::I1(i8::try_from(self)?)),
            (_, StateVarType::I2) => Ok(StateValue::I2(i16::try_from(self)?)),
            (_, StateVarType::I4) => Ok(StateValue::I4(i32::try_from(self)?)),
            (_, StateVarType::Int) => Ok(StateValue::Int(i32::try_from(self)?)),

            (_, StateVarType::R8) => Ok(StateValue::R8(f64::try_from(self)?)),
            (_, StateVarType::Number) => Ok(StateValue::Number(f64::try_from(self)?)),
            (_, StateVarType::Fixed14_4) => Ok(StateValue::Fixed14_4(f64::try_from(self)?)),
            (_, StateVarType::R4) => Ok(StateValue::R4(f32::try_from(self)?)),

            // --- Pas encore implémenté pour les autres types ---
            (val, target) => Err(StateValueError::TypeError(format!(
                "Cannot cast {:?} to {:?}",
                val, target
            ))),
        }
    }
}
