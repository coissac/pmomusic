use crate::variable_types::{StateVarType, type_trait::UpnpVarType};

impl UpnpVarType for StateVarType {
    fn as_state_var_type(&self) -> StateVarType {
        *self
    }

    fn bit_size(&self) -> Option<usize> {
        match self {
            StateVarType::UI1 | StateVarType::I1 => Some(8),
            StateVarType::UI2 | StateVarType::I2 => Some(16),
            StateVarType::UI4 | StateVarType::I4 | StateVarType::Int | StateVarType::R4 => Some(32),
            StateVarType::R8 | StateVarType::Number | StateVarType::Fixed14_4 => Some(64),
            _ => None,
        }
    }

    fn is_numeric(&self) -> bool {
        matches!(
            self,
            StateVarType::UI1
                | StateVarType::UI2
                | StateVarType::UI4
                | StateVarType::I1
                | StateVarType::I2
                | StateVarType::I4
                | StateVarType::Int
                | StateVarType::R4
                | StateVarType::R8
                | StateVarType::Number
                | StateVarType::Fixed14_4
        )
    }

    fn is_integer(&self) -> bool {
        matches!(
            self,
            StateVarType::UI1
                | StateVarType::UI2
                | StateVarType::UI4
                | StateVarType::I1
                | StateVarType::I2
                | StateVarType::I4
                | StateVarType::Int
        )
    }

    fn is_signed_int(&self) -> bool {
        matches!(
            self,
            StateVarType::I1 | StateVarType::I2 | StateVarType::I4 | StateVarType::Int
        )
    }

    fn is_unsigned_int(&self) -> bool {
        matches!(
            self,
            StateVarType::UI1 | StateVarType::UI2 | StateVarType::UI4
        )
    }

    fn is_float(&self) -> bool {
        matches!(
            self,
            StateVarType::R4 | StateVarType::R8 | StateVarType::Number | StateVarType::Fixed14_4
        )
    }

    fn is_bool(&self) -> bool {
        matches!(self, StateVarType::Boolean)
    }

    fn is_string(&self) -> bool {
        matches!(
            self,
            StateVarType::String
                | StateVarType::Char
                | StateVarType::BinHex
                | StateVarType::BinBase64
        )
    }

    fn is_time(&self) -> bool {
        matches!(
            self,
            StateVarType::Date
                | StateVarType::DateTime
                | StateVarType::DateTimeTZ
                | StateVarType::Time
                | StateVarType::TimeTZ
        )
    }

    fn is_uuid(&self) -> bool {
        matches!(self, StateVarType::UUID)
    }

    fn is_uri(&self) -> bool {
        matches!(self, StateVarType::URI)
    }

    fn is_binary(&self) -> bool {
        matches!(self, StateVarType::BinBase64 | StateVarType::BinHex)
    }

    fn is_comparable(&self) -> bool {
        !self.is_binary()
    }
}
