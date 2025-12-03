//! Implémentation de DeviceInstance.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use tracing::info;
use xmltree::{Element, EmitterConfig, XMLNode};

use crate::{
    UpnpInstance, UpnpObject, UpnpObjectType, UpnpTyped, UpnpTypedInstance,
    devices::{Device, errors::DeviceError},
    services::ServiceInstance,
};

const DEFAULT_NOTIFY_INTERVAL: Duration = Duration::from_secs(1);

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
        let device_type = model.device_category();
        let udn = match pmoconfig::get_config().get_device_udn(&device_type, device_name) {
            Ok(config_udn) => Self::normalize_udn(config_udn),
            Err(err) => {
                tracing::warn!(
                    "Failed to get/save UDN from config ({err:?}), using generated UUID"
                );
                Self::normalize_udn(uuid::Uuid::new_v4().to_string())
            }
        };

        // Obtenir l'IP locale et le port depuis la configuration
        // TODO: c'est amusant cet instanciation sauvage de base_url
        let local_ip = pmoutils::guess_local_ip();
        let port = pmoconfig::get_config().get_http_port();
        let server_base_url = format!("http://{}:{}", local_ip, port);

        Self {
            object: UpnpObjectType {
                name: model.get_name().to_string(),
                object_type: "DeviceInstance".to_string(),
            },
            model: Arc::new(model.clone()),
            udn,
            server_base_url,
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
        device_type
            .children
            .push(XMLNode::Text(self.model.device_type()));
        elem.children.push(XMLNode::Element(device_type));

        // friendlyName
        let mut friendly_name = Element::new("friendlyName");
        friendly_name
            .children
            .push(XMLNode::Text(self.model.friendly_name().to_string()));
        elem.children.push(XMLNode::Element(friendly_name));

        // manufacturer
        let mut manufacturer = Element::new("manufacturer");
        manufacturer
            .children
            .push(XMLNode::Text(self.model.manufacturer().to_string()));
        elem.children.push(XMLNode::Element(manufacturer));

        // modelName
        let mut model_name = Element::new("modelName");
        model_name
            .children
            .push(XMLNode::Text(self.model.model_name().to_string()));
        elem.children.push(XMLNode::Element(model_name));

        // UDN
        let mut udn = Element::new("UDN");
        udn.children.push(XMLNode::Text(self.udn_with_prefix()));
        elem.children.push(XMLNode::Element(udn));

        // serviceList
        let services = self.services.read().unwrap();
        if !services.is_empty() {
            let mut service_list = Element::new("serviceList");
            for service in services.values() {
                service_list
                    .children
                    .push(XMLNode::Element(service.to_xml_element()));
            }
            elem.children.push(XMLNode::Element(service_list));
        }

        // deviceList (sous-devices)
        let devices = self.devices.read().unwrap();
        if !devices.is_empty() {
            let mut device_list = Element::new("deviceList");
            for device in devices.values() {
                device_list
                    .children
                    .push(XMLNode::Element(device.to_xml_element()));
            }
            elem.children.push(XMLNode::Element(device_list));
        }

        // presentationURL
        if let Some(url) = self.model.presentation_url() {
            let mut presentation_url = Element::new("presentationURL");
            presentation_url
                .children
                .push(XMLNode::Text(url.to_string()));
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

    /// Retourne l'UDN avec le préfixe `uuid:` requis par la spécification.
    pub fn udn_with_prefix(&self) -> String {
        if self.udn.starts_with("uuid:") {
            self.udn.clone()
        } else {
            format!("uuid:{}", self.udn)
        }
    }

    /// Retourne l'URL de base du serveur (protocole + host + port).
    pub fn base_url(&self) -> &str {
        &self.server_base_url
    }

    /// Retourne la route du device (chemin relatif).
    /// Utilise l'UDN pour garantir l'unicité si plusieurs devices du même type existent.
    pub fn route(&self) -> String {
        format!("/device/{}", self.udn())
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
    pub fn register_urls<'a>(
        &'a self,
        server: &'a mut pmoserver::Server,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), DeviceError>> + 'a>> {
        Box::pin(async move {
            info!(
                "✅ Device description for {} available at: {}{}",
                self.get_name(),
                self.base_url(),
                self.description_route(),
            );

            // Handler pour la description du device
            let instance_desc = self.clone();
            server
                .add_handler(&self.description_route(), move || {
                    let instance = instance_desc.clone();
                    async move { instance.description_handler().await }
                })
                .await;

            // Enregistrer les services
            for service in self.services() {
                service
                    .register_urls(server)
                    .await
                    .map_err(|e| DeviceError::UrlRegistrationError(e.to_string()))?;
                // Start the periodic notifier so buffered state changes are flushed to subscribers.
                let _ = service.start_notifier(DEFAULT_NOTIFY_INTERVAL);
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
        tracing::info!("📋 Device description requested for {}", self.get_name());

        let elem = self.description_element();

        let config = EmitterConfig::new()
            .perform_indent(true)
            .indent_string("  ");

        let mut xml_output = Vec::new();
        if let Err(e) = elem.write_with_config(&mut xml_output, config) {
            tracing::error!("❌ Failed to serialize device description XML: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        let xml = String::from_utf8_lossy(&xml_output).to_string();

        tracing::debug!("✅ Device description generated ({} bytes)", xml.len());

        (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/xml; charset=\"utf-8\"",
            )],
            xml,
        )
            .into_response()
    }

    /// Crée un SsdpDevice configuré pour ce device UPnP.
    ///
    /// Cette méthode simplifie la création d'un device SSDP en configurant automatiquement :
    /// - L'UDN du device
    /// - Le type de device
    /// - La location (URL de description)
    /// - Le serveur (User-Agent avec OS/version détecté automatiquement)
    /// - Les types de notification pour tous les services
    ///
    /// # Arguments
    ///
    /// * `app_name` - Nom de l'application (ex: "PMOMusic")
    /// * `app_version` - Version de l'application (ex: "1.0")
    ///
    /// # Exemple
    ///
    /// ```ignore
    /// let renderer_instance = MEDIA_RENDERER.create_instance();
    /// let ssdp_device = renderer_instance.to_ssdp_device("PMOMusic", "1.0");
    /// ssdp_server.add_device(ssdp_device);
    /// ```
    pub fn to_ssdp_device(&self, app_name: &str, app_version: &str) -> crate::ssdp::SsdpDevice {
        let location = format!("{}{}", self.base_url(), self.description_route());
        let os_string = pmoutils::get_os_string();
        let server_string = format!("{} UPnP/1.1 {}/{}", os_string, app_name, app_version);

        let mut ssdp_device = crate::ssdp::SsdpDevice::new(
            self.udn().to_string(),
            self.model.device_type(),
            location,
            server_string,
        );

        // Ajouter les types de notification pour chaque service
        for service in self.services() {
            ssdp_device.add_notification_type(service.service_type());
        }

        ssdp_device
    }

    fn normalize_udn<S: Into<String>>(raw: S) -> String {
        let value: String = raw.into();
        let trimmed = value.trim();
        let sanitized = trimmed.strip_prefix("uuid:").unwrap_or(trimmed);
        sanitized.to_string()
    }
}
