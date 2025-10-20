mod cast;
mod default_value;
mod display_type;
mod display_value;
mod errors;
mod fromstr;
mod reflect_impl;
mod type_methods;
mod type_trait;
mod value_methods;
mod value_trait;

mod values_from_type;

mod values_from_i16;
mod values_from_i32;
mod values_from_i64;
mod values_from_i8;
mod values_from_u16;
mod values_from_u32;
mod values_from_u8;

mod values_from_f32;
mod values_from_f64;

mod values_from_datetime;
mod values_from_naivedate;
mod values_from_naivedatetime;
mod values_from_naivetime;
mod values_from_uri;
mod values_from_uuid;
mod values_from_vec_u8;

mod values_from_str;

use std::fmt::Debug;

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime};
use url::Url;
use uuid::Uuid;

pub use errors::StateValueError;
pub use type_trait::UpnpVarType;

pub use crate::variable_types::value_trait::UpnpValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateVarType {
    UI1,        // Unsigned 8-bit integer
    UI2,        // Unsigned 16-bit integer
    UI4,        // Unsigned 32-bit integer
    I1,         // Signed 8-bit integer
    I2,         // Signed 16-bit integer
    I4,         // Signed 32-bit integer
    Int,        // Synonymous with i4
    R4,         // 32-bit floating point
    R8,         // 64-bit floating point
    Number,     // Synonymous with r8
    Fixed14_4,  // Fixed-point decimal
    Char,       // Single Unicode character
    String,     // Character string
    Boolean,    // Boolean value
    BinBase64,  // Base64-encoded binary
    BinHex,     // Hex-encoded binary
    Date,       // Date (YYYY-MM-DD)
    DateTime,   // DateTime without timezone
    DateTimeTZ, // DateTime with timezone
    Time,       // Time without timezone
    TimeTZ,     // Time with timezone
    UUID,       // Universally unique identifier
    URI,        // Uniform Resource Identifier
}

#[derive(Clone, Debug)]
pub enum StateValue {
    UI1(u8),
    UI2(u16),
    UI4(u32),
    I1(i8),
    I2(i16),
    I4(i32),
    Int(i32),
    R4(f32),
    R8(f64),
    Number(f64),
    Fixed14_4(f64),
    Char(char),
    String(String),
    Boolean(bool),
    BinBase64(String),
    BinHex(String),
    Date(NaiveDate),
    DateTime(NaiveDateTime),
    DateTimeTZ(DateTime<FixedOffset>),
    Time(NaiveTime),
    TimeTZ(DateTime<FixedOffset>),
    UUID(Uuid),
    URI(Url),
}
