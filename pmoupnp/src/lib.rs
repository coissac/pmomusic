mod object_set;
mod object_trait;

pub mod actions;
pub mod cache_registry;
pub mod config_ext;
pub mod devices;
pub mod services;
pub mod soap;
pub mod ssdp;
pub mod state_variables;
pub mod upnp_api;
pub mod upnp_server;
pub mod value_ranges;
pub mod variable_types;

use std::sync::RwLock;
use std::{collections::HashMap, sync::Arc};

// Réexports pour compatibilité (seulement get_*, pas register_*)
pub use pmoaudiocache::get_audio_cache;
pub use pmocovers::get_cover_cache;

pub use crate::config_ext::UpnpConfigExt;
pub use crate::object_trait::*;
pub use crate::upnp_server::UpnpServerExt;

#[derive(Debug, Clone)]
pub struct UpnpObjectType {
    name: String,
    object_type: String,
}

#[derive(Debug)]
pub struct UpnpObjectSet<T: UpnpTypedObject> {
    objects: RwLock<HashMap<String, Arc<T>>>,
    order: RwLock<Vec<String>>,
}

#[derive(Debug)]
pub enum UpnpObjectSetError {
    AlreadyExists(String),
}
