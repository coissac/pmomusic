//! Module pour les devices UPnP.
//!
//! Ce module fournit les structures et fonctionnalites pour creer et gerer
//! des devices UPnP selon la specification UPnP Device Architecture.
//!
//! # Architecture
//!
//! - [`Device`] : Modele d'un device UPnP
//! - [`DeviceInstance`] : Instance concrete d'un device
//! - [`DeviceError`](errors::DeviceError) : Erreurs liees aux devices
//!
//! # Exemple
//!
//! ```ignore
//! use pmoupnp::devices::Device;
//! use pmoupnp::services::Service;
//! use std::sync::Arc;
//!
//! // Creer un device MediaRenderer
//! let mut device = Device::new(
//!     "MediaRenderer".to_string(),
//!     "MediaRenderer".to_string(),
//!     "PMOMusic Renderer".to_string()
//! );
//!
//! // Ajouter des services
//! let avtransport = Arc::new(Service::new("AVTransport".to_string()));
//! device.add_service(avtransport).unwrap();
//!
//! // Creer une instance
//! let instance = device.create_instance();
//! ```

mod device;
mod device_instance;
mod device_methods;
mod device_registry;
pub mod errors;

pub use device::Device;
pub use device_instance::DeviceInstance;
pub use device_registry::{DeviceRegistry, DeviceInstanceSet, DeviceInfo, ServiceInfo, ActionInfo, ArgumentInfo, VariableInfo};
