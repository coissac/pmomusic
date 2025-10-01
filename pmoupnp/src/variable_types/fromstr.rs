use crate::variable_types::StateVarType;
use std::str::FromStr;

impl FromStr for StateVarType {
    type Err = String; // Type d'erreur personnalisé

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ui1" => Ok(StateVarType::UI1),
            "ui2" => Ok(StateVarType::UI2),
            "ui4" => Ok(StateVarType::UI4),
            "i1" => Ok(StateVarType::I1),
            "i2" => Ok(StateVarType::I2),
            "i4" => Ok(StateVarType::I4),
            "int" => Ok(StateVarType::Int),
            "r4" => Ok(StateVarType::R4),
            "r8" => Ok(StateVarType::R8),
            "number" => Ok(StateVarType::Number),
            "fixed.14.4" => Ok(StateVarType::Fixed14_4),
            "char" => Ok(StateVarType::Char),
            "string" => Ok(StateVarType::String),
            "boolean" => Ok(StateVarType::Boolean),
            "bin.base64" => Ok(StateVarType::BinBase64),
            "bin.hex" => Ok(StateVarType::BinHex),
            "date" => Ok(StateVarType::Date),
            "datetime" => Ok(StateVarType::DateTime),
            "datetime.tz" => Ok(StateVarType::DateTimeTZ),
            "time" => Ok(StateVarType::Time),
            "time.tz" => Ok(StateVarType::TimeTZ),
            "uuid" => Ok(StateVarType::UUID),
            "uri" => Ok(StateVarType::URI),
            _ => Err(format!("Type inconnu: {}", s)),
        }
    }
}
