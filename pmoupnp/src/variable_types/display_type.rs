use std::fmt;

use crate::variable_types::StateVarType;


impl fmt::Display for StateVarType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            StateVarType::UI1 => "ui1",
            StateVarType::UI2 => "ui2",
            StateVarType::UI4 => "ui4",
            StateVarType::I1 => "i1",
            StateVarType::I2 => "i2",
            StateVarType::I4 => "i4",
            StateVarType::Int => "int",
            StateVarType::R4 => "r4",
            StateVarType::R8 => "r8",
            StateVarType::Number => "number",
            StateVarType::Fixed14_4 => "fixed.14.4",
            StateVarType::Char => "char",
            StateVarType::String => "string",
            StateVarType::Boolean => "boolean",
            StateVarType::BinBase64 => "bin.base64",
            StateVarType::BinHex => "bin.hex",
            StateVarType::Date => "date",
            StateVarType::DateTime => "dateTime",
            StateVarType::DateTimeTZ => "dateTime.tz",
            StateVarType::Time => "time",
            StateVarType::TimeTZ => "time.tz",
            StateVarType::UUID => "uuid",
            StateVarType::URI => "uri",
        };
        write!(f, "{}", s)
    }
}
