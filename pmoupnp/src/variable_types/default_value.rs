use chrono::{DateTime, FixedOffset, NaiveDate, NaiveTime};
use url::Url;
use uuid::Uuid;

use crate::variable_types::{StateValue, StateVarType};

impl StateVarType {
    pub fn default_value(&self) -> StateValue {
        match self {
            StateVarType::UI1 => StateValue::UI1(0),
            StateVarType::UI2 => StateValue::UI2(0),
            StateVarType::UI4 => StateValue::UI4(0),
            StateVarType::I1 => StateValue::I1(0),
            StateVarType::I2 => StateValue::I2(0),
            StateVarType::I4 => StateValue::I4(0),
            StateVarType::Int => StateValue::Int(0),
            StateVarType::R4 => StateValue::R4(0.0),
            StateVarType::R8 => StateValue::R8(0.0),
            StateVarType::Number => StateValue::Number(0.0),
            StateVarType::Fixed14_4 => StateValue::Fixed14_4(0.0),
            StateVarType::Char => StateValue::Char('\0'),
            StateVarType::String => StateValue::String(String::new()),
            StateVarType::Boolean => StateValue::Boolean(false),
            StateVarType::BinBase64 => StateValue::BinBase64(String::new()),
            StateVarType::BinHex => StateValue::BinHex(String::new()),
            StateVarType::Date => StateValue::Date(NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()),
            StateVarType::DateTime => StateValue::DateTime(DateTime::from_timestamp(0, 0).unwrap().naive_utc().into()),
            StateVarType::DateTimeTZ => StateValue::DateTimeTZ(
                DateTime::from_naive_utc_and_offset(
                    DateTime::from_timestamp(0, 0).unwrap().naive_utc(), 
                    FixedOffset::east_opt(0).unwrap())
            ),
            StateVarType::Time => StateValue::Time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
            StateVarType::TimeTZ => StateValue::TimeTZ(
                DateTime::from_naive_utc_and_offset(
                    DateTime::from_timestamp(0, 0).unwrap().naive_utc(), 
                    FixedOffset::east_opt(0).unwrap())
            ),
            StateVarType::UUID => StateValue::UUID(Uuid::nil()),
            StateVarType::URI => StateValue::URI(Url::parse("http://localhost").unwrap()),
        }
    }
}