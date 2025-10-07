mod object_trait;
mod object_set;
mod server;

pub mod actions;
pub mod devices;
pub mod mediarenderer;
pub mod services;
pub mod soap;
pub mod ssdp;
pub mod state_variables;
pub mod value_ranges;
pub mod variable_types;




use std::{collections::HashMap, sync::Arc};

use std::sync::RwLock;

pub use crate::object_trait::*;
pub use crate::server::UpnpServer;

#[derive(Debug, Clone)]
pub struct UpnpObjectType {
    name: String,
    object_type: String,
}

#[derive(Debug)]
pub struct UpnpObjectSet<T: UpnpTypedObject> {
    objects: RwLock<HashMap<String, Arc<T>>>,
}

#[derive(Debug)]
pub enum UpnpObjectSetError {
    AlreadyExists(String),
}

