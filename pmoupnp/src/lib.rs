mod object_trait;
mod object_set;

pub mod actions;
pub mod mediarenderer;
pub mod server;
// pub mod services;
pub mod state_variables;
pub mod value_ranges;
pub mod variable_types;


use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

pub use crate::object_trait::*;

#[derive(Debug, Clone)]
pub struct UpnpObjectType {
    name: String,
    object_type: String,
}

#[derive(Debug)]
pub struct UpnpObjectSet<T: UpnpTypedObject> {
    objects: RwLock<HashMap<String, Arc<T>>>,
}

pub enum UpnpObjectSetError {
    AlreadyExists(String),
}

