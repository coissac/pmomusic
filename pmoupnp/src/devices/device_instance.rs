//! Implémentation de DeviceInstance.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::info;
use xmltree::{Element, XMLNode, EmitterConfig};

use crate::{
    devices::{Device, errors::DeviceError},
    services::ServiceInstance,
    UpnpObject, UpnpInstance, UpnpTyped, UpnpTypedInstance, UpnpObjectType,
};

/// Instance d'un device UPnP.
///
/// Représente une instance concrète d'un device UPnP, avec ses services instanciés
/// et son UDN unique.
pub struct DeviceInstance {
    /// Métadonnées de l'objet
    object: UpnpObjectType,

    /// Référence vers le modèle
    model: Arc<Device>,

    /// UDN unique pour cette instance
    udn: String,

    /// URL de base du serveur
    server_base_url: String,

    /// Instances de services
    services: RwLock<HashMap<String, Arc<ServiceInstance>>>,

    /// Instances de sous-devices
    devices: RwLock<HashMap<String, Arc<DeviceInstance>>>,
}

impl Clone for DeviceInstance {
    fn clone(&self) -> Self {
        Self {
            object: self.object.clone(),
            model: Arc::clone(&self.model),
            udn: self.udn.clone(),
            server_base_url: self.server_base_url.clone(),
            services: RwLock::new(self.services.read().unwrap().clone()),
            devices: RwLock::new(self.devices.read().unwrap().clone()),
        }
    }
}

impl std::fmt::Debug for DeviceInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceInstance")
            .field("object", &self.object)
            .field("udn", &self.udn)
            .field("server_base_url", &self.server_base_url)
            .field("services", &self.services)
            .field("devices", &self.devices)
            .finish()
    }
}

impl UpnpTyped for DeviceInstance {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        &self.object
    }
}

impl UpnpInstance for DeviceInstance {
    type Model = Device;

    fn new(model: &Device) -> Self {
        // Obtenir ou créer un UDN persistant via la configuration
        let device_name = model.get_name();

        let udn = if let Ok(config_udn) = pmoconfig::get_config().get_device_udn("mediarenderer", device_name) {
            config_udn
        } else {
            // Fallback : générer un UDN
            tracing::warn!("Failed to get/save UDN from config, using generated UUID");
            format!("uuid:{}_{}", model.udn_prefix(), uuid::Uuid::new_v4())
        };

        Self {
            object: UpnpObjectType {
                name: model.get_name().to_string(),
                object_type: "DeviceInstance".to_string(),
            },
            model: Arc::new(model.clone()),
            udn,
            server_base_url: "http://localhost:8080".to_string(),
            services: RwLock::new(HashMap::new()),
            devices: RwLock::new(HashMap::new()),
        }
    }
}

impl UpnpTypedInstance for DeviceInstance {
    fn get_model(&self) -> &Self::Model {
        &self.model
    }
}

impl UpnpObject for DeviceInstance {
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("device");

        // deviceType
        let mut device_type = Element::new("deviceType");
        device_type.children.push(XMLNode::Text(self.model.device_type()));
        elem.children.push(XMLNode::Element(device_type));

        // friendlyName
        let mut friendly_name = Element::new("friendlyName");
        friendly_name.children.push(XMLNode::Text(self.model.friendly_name().to_string()));
        elem.children.push(XMLNode::Element(friendly_name));

        // manufacturer
        let mut manufacturer = Element::new("manufacturer");
        manufacturer.children.push(XMLNode::Text(self.model.manufacturer().to_string()));
        elem.children.push(XMLNode::Element(manufacturer));

        // modelName
        let mut model_name = Element::new("modelName");
        model_name.children.push(XMLNode::Text(self.model.model_name().to_string()));
        elem.children.push(XMLNode::Element(model_name));

        // UDN
        let mut udn = Element::new("UDN");
        udn.children.push(XMLNode::Text(self.udn.clone()));
        elem.children.push(XMLNode::Element(udn));

        // serviceList
        let services = self.services.read().unwrap();
        if !services.is_empty() {
            let mut service_list = Element::new("serviceList");
            for service in services.values() {
                service_list.children.push(XMLNode::Element(service.to_xml_element()));
            }
            elem.children.push(XMLNode::Element(service_list));
        }

        // deviceList (sous-devices)
        let devices = self.devices.read().unwrap();
        if !devices.is_empty() {
            let mut device_list = Element::new("deviceList");
            for device in devices.values() {
                device_list.children.push(XMLNode::Element(device.to_xml_element()));
            }
            elem.children.push(XMLNode::Element(device_list));
        }

        // presentationURL
        if let Some(url) = self.model.presentation_url() {
            let mut presentation_url = Element::new("presentationURL");
            presentation_url.children.push(XMLNode::Text(url.to_string()));
            elem.children.push(XMLNode::Element(presentation_url));
        }

        elem
    }
}

impl DeviceInstance {
    /// Définit l'URL de base du serveur.
    pub fn set_server_base_url(&mut self, url: String) {
        self.server_base_url = url;
    }

    /// Retourne l'UDN du device.
    pub fn udn(&self) -> &str {
        &self.udn
    }

    /// Retourne l'URL de base du serveur (protocole + host + port).
    pub fn base_url(&self) -> &str {
        &self.server_base_url
    }

    /// Retourne la route du device (chemin relatif).
    pub fn route(&self) -> String {
        format!("/device/{}", self.get_name())
    }

    /// Retourne la route de description du device.
    pub fn description_route(&self) -> String {
        format!("{}/desc.xml", self.route())
    }

    /// Ajoute une instance de service au device.
    ///
    /// Cette méthode configure automatiquement le service pour qu'il connaisse
    /// son device parent.
    ///
    /// # Errors
    ///
    /// Retourne une erreur si un service avec le même nom existe déjà.
    pub fn add_service(self: &Arc<Self>, service: Arc<ServiceInstance>) -> Result<(), DeviceError> {
        let mut services = self.services.write().unwrap();
        let name = service.get_name().to_string();

        if services.contains_key(&name) {
            return Err(DeviceError::ServiceAlreadyExists(name));
        }

        // Configurer le service pour qu'il connaisse son device parent
        service.set_device(Arc::clone(self));

        services.insert(name, service);
        Ok(())
    }

    /// Retourne tous les services.
    pub fn services(&self) -> Vec<Arc<ServiceInstance>> {
        self.services.read().unwrap().values().cloned().collect()
    }

    /// Retourne un service par nom.
    pub fn get_service(&self, name: &str) -> Option<Arc<ServiceInstance>> {
        self.services.read().unwrap().get(name).cloned()
    }

    /// Ajoute une instance de sous-device.
    pub fn add_device(&self, device: Arc<DeviceInstance>) -> Result<(), DeviceError> {
        let mut devices = self.devices.write().unwrap();
        let name = device.get_name().to_string();

        if devices.contains_key(&name) {
            return Err(DeviceError::DeviceAlreadyExists(name));
        }

        devices.insert(name, device);
        Ok(())
    }

    /// Retourne tous les sous-devices.
    pub fn devices(&self) -> Vec<Arc<DeviceInstance>> {
        self.devices.read().unwrap().values().cloned().collect()
    }

    /// Enregistre toutes les URLs du device et de ses services dans le serveur.
    pub fn register_urls<'a>(&'a self, server: &'a mut crate::server::Server) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), DeviceError>> + 'a>> {
        Box::pin(async move {
            info!(
                "✅ Device description for {} available at: {}{}",
                self.get_name(),
                self.base_url(),
                self.description_route(),
            );

            // Handler pour la description du device
            let instance_desc = self.clone();
            server.add_handler(&self.description_route(), move || {
                let instance = instance_desc.clone();
                async move { instance.description_handler().await }
            }).await;

            // Enregistrer les services
            for service in self.services() {
                service.register_urls(server).await
                    .map_err(|e| DeviceError::UrlRegistrationError(e.to_string()))?;
            }

            // Enregistrer les sous-devices
            for device in self.devices() {
                device.register_urls(server).await?;
            }

            Ok(())
        })
    }

    /// Génère l'élément XML de description du device.
    pub fn description_element(&self) -> Element {
        let mut root = Element::new("root");
        root.attributes.insert(
            "xmlns".to_string(),
            "urn:schemas-upnp-org:device-1-0".to_string(),
        );

        // specVersion
        let mut spec = Element::new("specVersion");
        let mut major = Element::new("major");
        major.children.push(XMLNode::Text("1".to_string()));
        spec.children.push(XMLNode::Element(major));

        let mut minor = Element::new("minor");
        minor.children.push(XMLNode::Text("0".to_string()));
        spec.children.push(XMLNode::Element(minor));

        root.children.push(XMLNode::Element(spec));

        // device
        root.children.push(XMLNode::Element(self.to_xml_element()));

        root
    }

    /// Handler HTTP pour la description du device.
    async fn description_handler(&self) -> Response {
        let elem = self.description_element();

        let config = EmitterConfig::new()
            .perform_indent(true)
            .indent_string("  ");

        let mut xml_output = Vec::new();
        if let Err(e) = elem.write_with_config(&mut xml_output, config) {
            tracing::error!("Failed to serialize device description XML: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        let mut xml = String::from_utf8_lossy(&xml_output).to_string();

        // Ajouter l'en-tête XML
        xml.insert_str(0, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");

        (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/xml; charset=\"utf-8\"")],
            xml,
        ).into_response()
    }
}
