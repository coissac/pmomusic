//! Définition du modèle Device UPnP.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::{UpnpObjectType, UpnpTyped, services::Service};

use super::errors::DeviceError;

/// Modèle d'un device UPnP.
///
/// Représente la définition d'un device selon la spécification UPnP Device Architecture.
/// Un device peut contenir plusieurs services et éventuellement des sous-devices.
#[derive(Debug)]
pub struct Device {
    /// Métadonnées de l'objet
    object: UpnpObjectType,

    /// Type de device UPnP (ex: "MediaRenderer", "MediaServer")
    device_type: String,

    /// Version du device
    version: u8,

    /// Nom convivial du device
    friendly_name: String,

    /// Fabricant
    manufacturer: String,

    /// URL du fabricant
    manufacturer_url: Option<String>,

    /// Description du modèle
    model_description: Option<String>,

    /// Nom du modèle
    model_name: String,

    /// Numéro du modèle
    model_number: Option<String>,

    /// URL du modèle
    model_url: Option<String>,

    /// Numéro de série
    serial_number: Option<String>,

    /// UDN (Unique Device Name) - sera généré à l'instance
    udn_prefix: String,

    /// UPC (Universal Product Code)
    upc: Option<String>,

    /// URL de l'icône
    icon_url: Option<String>,

    /// URL de présentation
    presentation_url: Option<String>,

    /// Services du device
    services: RwLock<HashMap<String, Arc<Service>>>,

    /// Sous-devices (embedded devices)
    devices: RwLock<HashMap<String, Arc<Device>>>,
}

impl Clone for Device {
    fn clone(&self) -> Self {
        Self {
            object: self.object.clone(),
            device_type: self.device_type.clone(),
            version: self.version,
            friendly_name: self.friendly_name.clone(),
            manufacturer: self.manufacturer.clone(),
            manufacturer_url: self.manufacturer_url.clone(),
            model_description: self.model_description.clone(),
            model_name: self.model_name.clone(),
            model_number: self.model_number.clone(),
            model_url: self.model_url.clone(),
            serial_number: self.serial_number.clone(),
            udn_prefix: self.udn_prefix.clone(),
            upc: self.upc.clone(),
            icon_url: self.icon_url.clone(),
            presentation_url: self.presentation_url.clone(),
            services: RwLock::new(self.services.read().unwrap().clone()),
            devices: RwLock::new(self.devices.read().unwrap().clone()),
        }
    }
}

impl Device {
    /// Crée un nouveau modèle de device.
    ///
    /// # Arguments
    ///
    /// * `name` - Nom unique du device
    /// * `device_type` - Type UPnP du device
    /// * `friendly_name` - Nom convivial pour l'utilisateur
    pub fn new(name: String, device_type: String, friendly_name: String) -> Self {
        Self {
            object: UpnpObjectType {
                name: name.clone(),
                object_type: "Device".to_string(),
            },
            device_type,
            version: 1,
            friendly_name,
            manufacturer: "PMOMusic".to_string(),
            manufacturer_url: None,
            model_description: None,
            model_name: name.clone(),
            model_number: None,
            model_url: None,
            serial_number: None,
            udn_prefix: "pmomusic".to_string(),
            upc: None,
            icon_url: None,
            presentation_url: None,
            services: RwLock::new(HashMap::new()),
            devices: RwLock::new(HashMap::new()),
        }
    }

    /// Retourne le type de device UPnP.
    ///
    /// Format: `urn:schemas-upnp-org:device:{type}:{version}`
    pub fn device_type(&self) -> String {
        format!(
            "urn:schemas-upnp-org:device:{}:{}",
            self.device_type, self.version
        )
    }

    pub fn device_category(&self) -> &String {
        &self.device_type
    }

    /// Définit la version du device.
    pub fn set_version(&mut self, version: u8) -> Result<(), DeviceError> {
        if version == 0 {
            return Err(DeviceError::InvalidVersion);
        }
        self.version = version;
        Ok(())
    }

    /// Retourne la version du device.
    pub fn version(&self) -> u8 {
        self.version
    }

    /// Définit le fabricant.
    pub fn set_manufacturer(&mut self, manufacturer: String) {
        self.manufacturer = manufacturer;
    }

    /// Définit l'URL du fabricant.
    pub fn set_manufacturer_url(&mut self, url: String) {
        self.manufacturer_url = Some(url);
    }

    /// Définit la description du modèle.
    pub fn set_model_description(&mut self, description: String) {
        self.model_description = Some(description);
    }

    /// Définit le nom du modèle.
    pub fn set_model_name(&mut self, name: String) {
        self.model_name = name;
    }

    /// Définit le numéro du modèle.
    pub fn set_model_number(&mut self, number: String) {
        self.model_number = Some(number);
    }

    /// Définit le numéro de série.
    pub fn set_serial_number(&mut self, serial: String) {
        self.serial_number = Some(serial);
    }

    /// Définit le préfixe UDN.
    pub fn set_udn_prefix(&mut self, prefix: String) {
        self.udn_prefix = prefix;
    }

    /// Retourne le préfixe UDN.
    pub fn udn_prefix(&self) -> &str {
        &self.udn_prefix
    }

    /// Définit l'URL de présentation.
    pub fn set_presentation_url(&mut self, url: String) {
        self.presentation_url = Some(url);
    }

    /// Ajoute un service au device.
    ///
    /// # Errors
    ///
    /// Retourne une erreur si un service avec le même nom existe déjà.
    pub fn add_service(&self, service: Arc<Service>) -> Result<(), DeviceError> {
        let mut services = self.services.write().unwrap();
        let name = service.get_name().to_string();

        if services.contains_key(&name) {
            return Err(DeviceError::ServiceAlreadyExists(name));
        }

        services.insert(name, service);
        Ok(())
    }

    /// Retourne tous les services.
    pub fn services(&self) -> Vec<Arc<Service>> {
        self.services.read().unwrap().values().cloned().collect()
    }

    /// Retourne un service par nom.
    pub fn get_service(&self, name: &str) -> Option<Arc<Service>> {
        self.services.read().unwrap().get(name).cloned()
    }

    /// Ajoute un sous-device.
    ///
    /// # Errors
    ///
    /// Retourne une erreur si un device avec le même nom existe déjà.
    pub fn add_device(&self, device: Arc<Device>) -> Result<(), DeviceError> {
        let mut devices = self.devices.write().unwrap();
        let name = device.get_name().to_string();

        if devices.contains_key(&name) {
            return Err(DeviceError::DeviceAlreadyExists(name));
        }

        devices.insert(name, device);
        Ok(())
    }

    /// Retourne tous les sous-devices.
    pub fn devices(&self) -> Vec<Arc<Device>> {
        self.devices.read().unwrap().values().cloned().collect()
    }

    /// Retourne le nom convivial.
    pub fn friendly_name(&self) -> &str {
        &self.friendly_name
    }

    /// Retourne le fabricant.
    pub fn manufacturer(&self) -> &str {
        &self.manufacturer
    }

    /// Retourne le nom du modèle.
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Retourne la description du modèle.
    pub fn model_description(&self) -> Option<&str> {
        self.model_description.as_deref()
    }

    /// Retourne l'URL de présentation.
    pub fn presentation_url(&self) -> Option<&str> {
        self.presentation_url.as_deref()
    }

    /// Retourne l'URL du fabricant.
    pub fn manufacturer_url(&self) -> Option<&str> {
        self.manufacturer_url.as_deref()
    }

    /// Retourne le numéro du modèle.
    pub fn model_number(&self) -> Option<&str> {
        self.model_number.as_deref()
    }

    /// Retourne l'URL du modèle.
    pub fn model_url(&self) -> Option<&str> {
        self.model_url.as_deref()
    }

    /// Retourne le numéro de série.
    pub fn serial_number(&self) -> Option<&str> {
        self.serial_number.as_deref()
    }

    /// Retourne l'UPC.
    pub fn upc(&self) -> Option<&str> {
        self.upc.as_deref()
    }

    /// Retourne l'URL de l'icône.
    pub fn icon_url(&self) -> Option<&str> {
        self.icon_url.as_deref()
    }
}

impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Device({}:{})", self.get_name(), self.version)
    }
}

impl UpnpTyped for Device {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        &self.object
    }
}
