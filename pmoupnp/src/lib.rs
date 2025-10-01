mod object_trait;

pub mod server;
pub mod state_variables;
pub mod value_ranges;
pub mod variable_types;
pub mod actions;

pub use crate::object_trait::{UpnpXml,UpnpObject};

#[derive(Debug,Clone)]
pub struct UpnpObjectType {
    name: String,
    object_type: String,
}
