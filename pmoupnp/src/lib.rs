mod object_trait;

pub mod server;
pub mod state_variables;
pub mod value_ranges;
pub mod variable_types;

pub use crate::object_trait::UpnpObject;

#[derive(Clone)]
pub struct UpnpObjectType {
    name: String,
    object_type: String,
}
