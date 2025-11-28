//! Implémentation de ServiceInstance.
//!
//! Ce module contient l'implémentation complète de [`ServiceInstance`],
//! qui représente une instance active d'un service UPnP.
//!
//! # Composants principaux
//!
//! - [`ServiceInstance`] : Structure principale contenant l'état du service
//! - [`event_sub_handler`] : Handler Axum pour les abonnements aux événements
//! - [`control_handler`] : Handler Axum pour les appels SOAP
//!
//! # Gestion des événements
//!
//! L'instance gère automatiquement :
//! - Les souscriptions aux événements (SUBSCRIBE/UNSUBSCRIBE)
//! - L'envoi d'événements initiaux aux nouveaux abonnés
//! - Les notifications périodiques des changements d'état
//! - Le séquençage des messages par abonné
//!
//! # Architecture
//!
//! ```text
//! ServiceInstance
//! ├── Variables d'état (StateVarInstanceSet)
//! ├── Actions (ActionInstanceSet)
//! ├── Abonnés (HashMap<SID, Callback>)
//! ├── Buffer de changements (Mutex<HashMap>)
//! └── Séquences (Mutex<HashMap<SID, u32>>)
//! ```

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bevy_reflect::Reflect;
use quick_xml::escape::escape;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};
use tokio::time;
use tracing::{debug, error, info, warn};
use xmltree::{Element, EmitterConfig, XMLNode};

use crate::{
    UpnpInstance, UpnpObject, UpnpObjectType, UpnpTyped, UpnpTypedInstance,
    actions::{ActionInstance, ActionInstanceSet},
    devices::DeviceInstance,
    services::{Service, ServiceError},
    state_variables::{StateVarInstance, StateVarInstanceSet, UpnpVariable},
};

/// Méthodes HTTP pour les événements UPnP.
pub const METHOD_SUBSCRIBE: &str = "SUBSCRIBE";
pub const METHOD_UNSUBSCRIBE: &str = "UNSUBSCRIBE";

/// Instance de service UPnP.
///
/// Représente une instance concrète d'un service UPnP, attachée à un device.
/// Gère l'exécution des actions, les notifications d'événements et les abonnements.
///
/// # Fonctionnalités
///
/// - Exécution d'actions via SOAP
/// - Gestion des abonnements aux événements (SUBSCRIBE/UNSUBSCRIBE)
/// - Notifications automatiques des changements d'état
/// - Génération de la description SCPD
///
/// # Cycle de vie
///
/// 1. Création via [`Service::create_instance`](crate::UpnpModel::create_instance)
/// 2. Enregistrement des URLs avec [`register_urls`](Self::register_urls)
/// 3. Démarrage du notifier avec [`start_notifier`](Self::start_notifier)
///
/// # Examples
///
/// ```rust,ignore
/// # use pmoupnp::services::Service;
/// # use pmoupnp::UpnpModel;
/// # use pmoserver::Server;
/// # use std::time::Duration;
/// # #[tokio::main]
/// # async fn main() {
/// let service = Service::new("AVTransport".to_string());
/// let instance = service.create_instance();
///
/// // Enregistrer les endpoints
/// let mut server = Server::new("test", "http://localhost:8080", 8080);
/// instance.register_urls(&mut server).await.unwrap();
///
/// // Démarrer les notifications
/// let _handle = instance.start_notifier(Duration::from_secs(5));
/// # }
/// ```
#[derive(Clone)]
pub struct ServiceInstance {
    /// Métadonnées de l'objet
    object: UpnpObjectType,

    /// Référence vers le modèle
    model: Arc<Service>,

    /// Identifiant du service
    identifier: String,

    /// Device parent (optionnel) - utilisé via interior mutability
    device: Arc<RwLock<Option<Arc<DeviceInstance>>>>,

    /// Variables d'état instanciées
    statevariables: StateVarInstanceSet,

    /// Actions instanciées
    actions: ActionInstanceSet,

    /// Abonnés aux événements (SID -> Callback URL)
    subscribers: Arc<RwLock<HashMap<String, String>>>,

    /// Buffer des changements en attente de notification (nom de variable -> valeur réflexive)
    changed_buffer: Arc<Mutex<HashMap<String, Arc<dyn Reflect>>>>,

    /// Compteurs de séquence par abonné
    seqid: Arc<Mutex<HashMap<String, u32>>>,
}

impl std::fmt::Debug for ServiceInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceInstance")
            .field("object", &self.object)
            .field("identifier", &self.identifier)
            .field("device", &self.device)
            .field("statevariables", &self.statevariables)
            .field("actions", &self.actions)
            .finish()
    }
}

impl UpnpTyped for ServiceInstance {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        &self.object
    }
}

impl UpnpInstance for ServiceInstance {
    type Model = Service;

    fn new(model: &Service) -> Self {
        // Phase 1 : Créer les instances de variables d'état
        let mut statevariables = StateVarInstanceSet::new();
        for v in model.variables() {
            if let Err(e) = statevariables.insert(Arc::new(StateVarInstance::new(&*v))) {
                error!("Failed to insert state variable: {:?}", e);
            }
        }

        // Phase 2 : Créer les instances d'actions avec validation
        let mut actions = ActionInstanceSet::new();
        for a in model.actions() {
            // Vérifier que toutes les variables référencées existent
            let mut missing_vars = Vec::new();

            for arg in a.arguments().all() {
                let related_var_name = arg.state_variable().get_name();
                if statevariables.get_by_name(related_var_name).is_none() {
                    missing_vars.push(related_var_name.to_string());
                }
            }

            if !missing_vars.is_empty() {
                error!(
                    "Action '{}' references missing state variables: {:?}",
                    a.get_name(),
                    missing_vars
                );
                continue;
            }

            // Créer l'instance d'action
            let action_instance = Arc::new(ActionInstance::new(&*a));

            // ✅ Phase 3 : ACTIVER le binding des arguments aux variables d'instance
            for arg_instance in action_instance.arguments_set().all() {
                let var_name = arg_instance.get_model().state_variable().get_name();
                if let Some(var_instance) = statevariables.get_by_name(var_name) {
                    // ✅ Activer cette ligne (déjà présente dans ArgumentInstance)
                    arg_instance.bind_variable(var_instance);
                }
            }

            if let Err(e) = actions.insert(action_instance) {
                error!("Failed to insert action '{}': {:?}", a.get_name(), e);
            }
        }

        Self {
            object: UpnpObjectType {
                name: model.name().to_string(),
                object_type: "ServiceInstance".to_string(),
            },
            model: Arc::new(model.clone()),
            identifier: model.identifier().to_string(),
            device: Arc::new(RwLock::new(None)),
            statevariables,
            actions,
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            changed_buffer: Arc::new(Mutex::new(HashMap::new())),
            seqid: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl UpnpTypedInstance for ServiceInstance {
    fn get_model(&self) -> &Self::Model {
        &self.model
    }
}

impl UpnpObject for ServiceInstance {
    fn to_xml_element(&self) -> Element {
        let mut elem = Element::new("service");

        let mut service_type = Element::new("serviceType");
        service_type
            .children
            .push(XMLNode::Text(self.service_type()));
        elem.children.push(XMLNode::Element(service_type));

        let mut service_id = Element::new("serviceId");
        service_id.children.push(XMLNode::Text(self.service_id()));
        elem.children.push(XMLNode::Element(service_id));

        let mut scpd_url = Element::new("SCPDURL");
        scpd_url.children.push(XMLNode::Text(self.scpd_route()));
        elem.children.push(XMLNode::Element(scpd_url));

        let mut control_url = Element::new("controlURL");
        control_url
            .children
            .push(XMLNode::Text(self.control_route()));
        elem.children.push(XMLNode::Element(control_url));

        let mut event_sub_url = Element::new("eventSubURL");
        event_sub_url
            .children
            .push(XMLNode::Text(self.event_route()));
        elem.children.push(XMLNode::Element(event_sub_url));

        elem
    }
}

impl ServiceInstance {
    /// Enregistre cette instance de service auprès de toutes ses variables.
    ///
    /// Cette méthode doit être appelée APRÈS la création de l'Arc<ServiceInstance>
    /// pour permettre aux variables de notifier le service lors de leurs changements.
    ///
    /// # Arguments
    ///
    /// * `self_arc` - Arc pointant vers cette instance
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// # use std::sync::Arc;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = Arc::new(service.create_instance());
    /// instance.register_with_variables(&instance);
    /// ```
    pub fn register_with_variables(self: &Arc<Self>) {
        let weak_self = Arc::downgrade(self);
        for var in self.statevariables.all() {
            var.register_service(weak_self.clone());
        }
    }

    /// Retourne l'identifiant du service.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = service.create_instance();
    /// assert_eq!(instance.identifier(), "AVTransport");
    /// ```
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Retourne le type de service UPnP.
    ///
    /// Format: `urn:schemas-upnp-org:service:{name}:{version}`
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = service.create_instance();
    /// assert_eq!(instance.service_type(), "urn:schemas-upnp-org:service:AVTransport:1");
    /// ```
    pub fn service_type(&self) -> String {
        self.model.service_type()
    }

    /// Retourne l'ID de service UPnP.
    ///
    /// Format: `urn:upnp-org:serviceId:{identifier}`
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = service.create_instance();
    /// assert_eq!(instance.service_id(), "urn:upnp-org:serviceId:AVTransport");
    /// ```
    pub fn service_id(&self) -> String {
        format!("urn:upnp-org:serviceId:{}", self.identifier)
    }

    /// Récupère une variable d'état par son nom.
    ///
    /// # Arguments
    ///
    /// * `name` - Nom de la variable d'état
    ///
    /// # Returns
    ///
    /// `Some(Arc<StateVarInstance>)` si la variable existe, `None` sinon.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// # let service = Service::new("AVTransport".to_string());
    /// # let instance = service.create_instance();
    /// if let Some(var) = instance.get_variable("TransportState") {
    ///     println!("Value: {}", var.value());
    /// }
    /// ```
    pub fn get_variable(&self, name: &str) -> Option<Arc<StateVarInstance>> {
        self.statevariables.get_by_name(name)
    }

    /// Récupère une action par son nom.
    ///
    /// # Arguments
    ///
    /// * `name` - Nom de l'action
    ///
    /// # Returns
    ///
    /// `Some(Arc<ActionInstance>)` si l'action existe, `None` sinon.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::{UpnpModel, UpnpTyped};
    /// # let service = Service::new("AVTransport".to_string());
    /// # let instance = service.create_instance();
    /// if let Some(action) = instance.get_action("Play") {
    ///     println!("Action found: {}", action.get_name());
    /// }
    /// ```
    pub fn get_action(&self, name: &str) -> Option<Arc<ActionInstance>> {
        self.actions.get_by_name(name)
    }

    /// Définit le device parent pour ce service.
    ///
    /// Cette méthode doit être appelée après la création du service instance
    /// pour établir la relation avec le device parent.
    ///
    /// # Arguments
    ///
    /// * `device` - Le device parent
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::devices::Device;
    /// # use pmoupnp::UpnpModel;
    /// # use std::sync::Arc;
    /// # let service = Service::new("AVTransport".to_string());
    /// # let device = Device::new("MediaRenderer".to_string(), "urn:schemas-upnp-org:device:MediaRenderer:1".to_string(), "My MediaRenderer".to_string());
    /// let service_instance = service.create_instance();
    /// let device_instance = device.create_instance();
    /// service_instance.set_device(device_instance);
    /// ```
    pub fn set_device(&self, device: Arc<DeviceInstance>) {
        let mut dev = self.device.write().unwrap();
        *dev = Some(device);
    }

    /// Retourne la route du service (chemin relatif).
    ///
    /// # Returns
    ///
    /// Chemin relatif incluant le device parent si présent.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = service.create_instance();
    /// assert_eq!(instance.route(), "/service/AVTransport");
    /// ```
    pub fn route(&self) -> String {
        let device = self.device.read().unwrap();
        match device.as_ref() {
            Some(device) => format!("{}/service/{}", device.route(), self.get_name()),
            None => format!("/service/{}", self.get_name()),
        }
    }

    /// Retourne la route de contrôle SOAP.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = service.create_instance();
    /// assert_eq!(instance.control_route(), "/service/AVTransport/control");
    /// ```
    pub fn control_route(&self) -> String {
        format!("{}/control", self.route())
    }

    /// Retourne la route de souscription aux événements.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = service.create_instance();
    /// assert_eq!(instance.event_route(), "/service/AVTransport/event");
    /// ```
    pub fn event_route(&self) -> String {
        format!("{}/event", self.route())
    }

    /// Retourne la route de la description SCPD.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = service.create_instance();
    /// assert_eq!(instance.scpd_route(), "/service/AVTransport/desc.xml");
    /// ```
    pub fn scpd_route(&self) -> String {
        format!("{}/desc.xml", self.route())
    }

    /// Retourne l'USN (Unique Service Name).
    ///
    /// L'USN combine l'UUID du device parent et le type de service UPnP.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = service.create_instance();
    /// let usn = instance.usn();
    /// // Format: uuid:{device-uuid}::urn:schemas-upnp-org:service:AVTransport:1
    /// ```
    pub fn usn(&self) -> String {
        let device = self.device.read().unwrap();
        match device.as_ref() {
            Some(device) => format!("{}::urn:{}", device.udn_with_prefix(), self.service_type()),
            None => format!("uuid::urn:{}", self.service_type()),
        }
    }

    /// Retourne une référence vers l'ensemble des variables d'état.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = service.create_instance();
    /// let vars = instance.statevariables();
    /// println!("Variables count: {}", vars.all().len());
    /// ```
    pub fn statevariables(&self) -> &StateVarInstanceSet {
        &self.statevariables
    }

    /// Retourne une référence vers l'ensemble des actions.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = service.create_instance();
    /// let actions = instance.actions();
    /// println!("Actions count: {}", actions.all().len());
    /// ```
    pub fn actions(&self) -> &ActionInstanceSet {
        &self.actions
    }

    /// Retourne une action par son nom.
    ///
    /// # Arguments
    ///
    /// * `name` - Nom de l'action
    ///
    /// # Returns
    ///
    /// `Some(Arc<ActionInstance>)` si trouvée, `None` sinon.
    pub fn action(&self, name: &str) -> Option<Arc<crate::actions::ActionInstance>> {
        self.actions.get_by_name(name)
    }

    /// Enregistre les routes UPnP dans le serveur.
    ///
    /// # Errors
    ///
    /// Retourne une erreur si l'enregistrement des routes échoue.
    pub async fn register_urls(&self, server: &mut pmoserver::Server) -> Result<(), ServiceError> {
        let device = self.device.read().unwrap();
        let device_name = device
            .as_ref()
            .map(|d| d.get_name().clone())
            .unwrap_or_else(|| "unknown".to_string());
        let server_url = device
            .as_ref()
            .map(|d| d.base_url().to_string())
            .unwrap_or_default();
        drop(device);

        info!(
            "✅ Service description for {}:{} available at : {}{}",
            device_name,
            self.get_name(),
            server_url,
            self.scpd_route(),
        );

        // Handler SCPD
        let instance_scpd = self.clone();
        server
            .add_handler(&self.scpd_route(), move || {
                let instance = instance_scpd.clone();
                async move { instance.scpd_handler().await }
            })
            .await;

        // Handler control
        let instance_control = Arc::new(self.clone());
        server
            .add_post_handler_with_state(&self.control_route(), control_handler, instance_control)
            .await;

        // Handler événements
        let instance_event = self.clone();
        server
            .add_handler_with_state(&self.event_route(), event_sub_handler, instance_event)
            .await;

        Ok(())
    }

    /// Génère l'élément XML SCPD (Service Control Protocol Description).
    ///
    /// Cette méthode crée un élément XML conforme à la spécification UPnP décrivant
    /// le service, ses actions et ses variables d'état.
    ///
    /// # Returns
    ///
    /// Un élément `xmltree::Element` représentant le document SCPD.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = service.create_instance();
    /// let scpd = instance.scpd_element();
    /// assert_eq!(scpd.name, "scpd");
    /// ```
    pub fn scpd_element(&self) -> Element {
        let mut elem = Element::new("scpd");
        elem.attributes.insert(
            "xmlns".to_string(),
            "urn:schemas-upnp-org:service-1-0".to_string(),
        );

        // specVersion
        let mut spec = Element::new("specVersion");
        let mut major = Element::new("major");
        major.children.push(XMLNode::Text("1".to_string()));
        spec.children.push(XMLNode::Element(major));

        let mut minor = Element::new("minor");
        minor.children.push(XMLNode::Text("0".to_string()));
        spec.children.push(XMLNode::Element(minor));

        elem.children.push(XMLNode::Element(spec));

        // actionList (depuis le modèle)
        if !self.model.actions.all().is_empty() {
            elem.children
                .push(XMLNode::Element(self.model.actions.to_xml_element()));
        }

        // serviceStateTable (depuis le modèle)
        if !self.model.state_table.all().is_empty() {
            elem.children
                .push(XMLNode::Element(self.model.state_table.to_xml_element()));
        }

        elem
    }

    /// Handler HTTP pour la description SCPD.
    ///
    /// Génère et retourne le document XML SCPD décrivant le service.
    /// Cette méthode est appelée lorsqu'un client accède à l'URL SCPD du service.
    ///
    /// # Returns
    ///
    /// Une réponse HTTP 200 avec le XML SCPD, ou 500 en cas d'erreur de sérialisation.
    ///
    /// # Format de réponse
    ///
    /// - Content-Type: `text/xml; charset="utf-8"`
    /// - Body: Document SCPD formaté avec indentation
    async fn scpd_handler(&self) -> Response {
        info!("📋 SCPD requested for service {}", self.get_name());

        let elem = self.scpd_element();

        let config = EmitterConfig::new()
            .perform_indent(true)
            .indent_string("  ");

        let mut xml_output = Vec::new();
        if let Err(e) = elem.write_with_config(&mut xml_output, config) {
            error!("❌ Failed to serialize SCPD XML: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        let xml = String::from_utf8_lossy(&xml_output).to_string();

        debug!(
            "✅ SCPD generated for {} ({} bytes)",
            self.get_name(),
            xml.len()
        );

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

    /// Ajoute un abonné aux événements.
    ///
    /// # Arguments
    ///
    /// * `sid` - Identifiant de la souscription (SID)
    /// * `callback` - URL de callback pour les notifications
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let service = Service::new("AVTransport".to_string());
    /// # let instance = service.create_instance();
    /// instance.add_subscriber(
    ///     "uuid:12345".to_string(),
    ///     "<http://192.168.1.100:8080/callback>".to_string()
    /// ).await;
    /// # }
    /// ```
    pub async fn add_subscriber(&self, sid: String, callback: String) {
        let mut subscribers = self.subscribers.write().unwrap();
        subscribers.insert(sid, callback);
    }

    /// Renouvelle un abonnement existant.
    ///
    /// # Arguments
    ///
    /// * `sid` - Identifiant de la souscription (SID)
    /// * `timeout` - Nouvelle durée de validité (format "Second-{n}")
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let service = Service::new("AVTransport".to_string());
    /// # let instance = service.create_instance();
    /// instance.renew_subscriber("uuid:12345", "Second-1800").await;
    /// # }
    /// ```
    pub async fn renew_subscriber(&self, sid: &str, timeout: &str) {
        info!("♻️ Renewed SID {} for timeout {}", sid, timeout);
    }

    /// Supprime un abonné.
    ///
    /// # Arguments
    ///
    /// * `sid` - Identifiant de la souscription (SID) à supprimer
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let service = Service::new("AVTransport".to_string());
    /// # let instance = service.create_instance();
    /// instance.remove_subscriber("uuid:12345").await;
    /// # }
    /// ```
    pub async fn remove_subscriber(&self, sid: &str) {
        let mut subscribers = self.subscribers.write().unwrap();
        subscribers.remove(sid);
    }

    /// Envoie l'événement initial à un nouvel abonné.
    ///
    /// Lorsqu'un client s'abonne aux événements, cette méthode lui envoie
    /// immédiatement les valeurs actuelles de toutes les variables d'état
    /// qui envoient des notifications.
    ///
    /// # Arguments
    ///
    /// * `sid` - Identifiant de la souscription (SID)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let service = Service::new("AVTransport".to_string());
    /// # let instance = service.create_instance();
    /// instance.send_initial_event("uuid:12345".to_string()).await;
    /// # }
    /// ```
    pub async fn send_initial_event(&self, sid: String) {
        let callback = {
            let subscribers = self.subscribers.read().unwrap();
            subscribers.get(&sid).cloned()
        };

        if let Some(callback) = callback {
            let mut changed = HashMap::new();
            for sv in self.statevariables.all() {
                if sv.is_sending_notification() {
                    changed.insert(sv.get_name().to_string(), sv.value().to_string());
                }
            }

            if changed.is_empty() {
                return;
            }

            tokio::spawn(async move {
                let callback = callback.trim().trim_matches(|c| c == '<' || c == '>');

                let mut body =
                    r#"<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">"#.to_string();
                for (name, val) in changed {
                    body.push_str(&format!(
                        "<e:property><{0}>{1}</{0}></e:property>",
                        name, val
                    ));
                }
                body.push_str("</e:propertyset>");

                let client = reqwest::Client::new();
                match client
                    .request(reqwest::Method::from_bytes(b"NOTIFY").unwrap(), callback)
                    .header("Content-Type", r#"text/xml; charset="utf-8"#)
                    .header("NT", "upnp:event")
                    .header("NTS", "upnp:propchange")
                    .header("SID", &sid)
                    .header("SEQ", "0")
                    .body(body)
                    .send()
                    .await
                {
                    Ok(resp) => {
                        info!(
                            "✅ Initial event sent to {}, status={}",
                            callback,
                            resp.status()
                        );
                    }
                    Err(e) => {
                        error!("Failed to send initial event to {}: {}", callback, e);
                    }
                }
            });
        }
    }

    /// Marque un changement de variable à notifier ultérieurement.
    ///
    /// Les changements sont mis en buffer et seront envoyés lors du prochain
    /// appel à [`notify_subscribers`](Self::notify_subscribers).
    ///
    /// # Arguments
    ///
    /// * `name` - Nom de la variable d'état modifiée
    /// * `value` - Nouvelle valeur de la variable
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// let service = Service::new("AVTransport".to_string());
    /// let instance = service.create_instance();
    /// let value = Arc::new("PLAYING".to_string()) as Arc<dyn Reflect>;
    /// instance.event_to_be_sent("TransportState".to_string(), value);
    /// ```
    pub fn event_to_be_sent(&self, name: String, value: Arc<dyn Reflect>) {
        let mut buffer = self.changed_buffer.lock().unwrap();
        buffer.insert(name, value);
    }

    /// Récupère le prochain numéro de séquence pour un abonné.
    ///
    /// Chaque notification envoyée à un abonné doit avoir un numéro de séquence
    /// unique et croissant.
    ///
    /// # Arguments
    ///
    /// * `sid` - Identifiant de la souscription (SID)
    ///
    /// # Returns
    ///
    /// Le prochain numéro de séquence sous forme de chaîne.
    fn next_seq(&self, sid: &str) -> String {
        let mut seqid = self.seqid.lock().unwrap();
        let counter = seqid.entry(sid.to_string()).or_insert(0);
        *counter += 1;
        counter.to_string()
    }

    /// Notifie tous les abonnés des changements en attente.
    ///
    /// Cette méthode envoie les changements bufferisés à tous les abonnés actuels
    /// via des requêtes HTTP NOTIFY. Les changements sont envoyés de manière
    /// asynchrone dans des tâches séparées.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::UpnpModel;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let service = Service::new("AVTransport".to_string());
    /// # let instance = service.create_instance();
    /// # use std::sync::Arc;
    /// # use bevy_reflect::Reflect;
    /// let value = Arc::new("PLAYING".to_string()) as Arc<dyn Reflect>;
    /// instance.event_to_be_sent("TransportState".to_string(), value);
    /// instance.notify_subscribers().await;
    /// # }
    /// ```
    pub async fn notify_subscribers(&self) {
        let subscribers_copy = {
            let subscribers = self.subscribers.read().unwrap();
            if subscribers.is_empty() {
                return;
            }
            subscribers.clone()
        };

        let changed = {
            let mut buffer = self.changed_buffer.lock().unwrap();
            if buffer.is_empty() {
                return;
            }
            std::mem::take(&mut *buffer)
        };

        for (sid, callback) in subscribers_copy {
            let changed_clone = changed.clone();
            let seq = self.next_seq(&sid);

            tokio::spawn(async move {
                let callback = callback.trim().trim_matches(|c| c == '<' || c == '>');

                let mut body =
                    r#"<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">"#.to_string();
                for (name, val) in changed_clone {
                    // Convertir la valeur Reflect en String
                    let val_str = Self::reflect_to_string(&*val);
                    body.push_str(&format!(
                        "<e:property><{0}>{1}</{0}></e:property>",
                        name, val_str
                    ));
                }
                body.push_str("</e:propertyset>");

                let client = reqwest::Client::new();
                match client
                    .request(reqwest::Method::from_bytes(b"NOTIFY").unwrap(), callback)
                    .header("Content-Type", r#"text/xml; charset="utf-8"#)
                    .header("NT", "upnp:event")
                    .header("NTS", "upnp:propchange")
                    .header("SID", &sid)
                    .header("SEQ", seq)
                    .body(body)
                    .send()
                    .await
                {
                    Ok(_) => {
                        info!("✅ Notified subscriber {} of changes", callback);
                    }
                    Err(e) => {
                        error!("Failed to notify subscriber {}: {}", callback, e);
                    }
                }
            });
        }
    }

    /// Convertit une valeur Reflect en String pour la notification UPnP.
    ///
    /// Cette fonction gère plusieurs cas :
    /// - Types primitifs : formatage direct
    /// - Structures serde (pmodidl, etc.) : sérialisation XML
    /// - Autres types : fallback sur Debug
    ///
    /// Le résultat est déjà échappé XML-safe selon les normes UPnP.
    fn reflect_to_string(value: &dyn Reflect) -> String {
        use bevy_reflect::ReflectRef;

        // Essayer de downcaster vers des types primitifs courants
        if let Some(v) = value.as_any().downcast_ref::<String>() {
            return v.clone();
        } else if let Some(v) = value.as_any().downcast_ref::<u8>() {
            return v.to_string();
        } else if let Some(v) = value.as_any().downcast_ref::<u16>() {
            return v.to_string();
        } else if let Some(v) = value.as_any().downcast_ref::<u32>() {
            return v.to_string();
        } else if let Some(v) = value.as_any().downcast_ref::<i8>() {
            return v.to_string();
        } else if let Some(v) = value.as_any().downcast_ref::<i16>() {
            return v.to_string();
        } else if let Some(v) = value.as_any().downcast_ref::<i32>() {
            return v.to_string();
        } else if let Some(v) = value.as_any().downcast_ref::<f32>() {
            return v.to_string();
        } else if let Some(v) = value.as_any().downcast_ref::<f64>() {
            return v.to_string();
        } else if let Some(v) = value.as_any().downcast_ref::<bool>() {
            return if *v { "1" } else { "0" }.to_string();
        } else if let Some(v) = value.as_any().downcast_ref::<char>() {
            return v.to_string();
        }

        // Pour les structures complexes, essayer de sérialiser avec bevy_reflect
        match value.reflect_ref() {
            ReflectRef::Struct(s) => {
                // Construire un XML simple pour la struct
                Self::serialize_struct_to_xml(s)
            }
            ReflectRef::TupleStruct(ts) => {
                // Pour les tuple structs, essayer d'extraire la valeur si c'est un wrapper
                if ts.field_len() == 1 {
                    if let Some(inner) = ts.field(0) {
                        // Convertir PartialReflect en Reflect si possible
                        if let Some(reflect_val) = inner.try_as_reflect() {
                            Self::reflect_to_string(reflect_val)
                        } else {
                            format!("{:?}", value)
                        }
                    } else {
                        format!("{:?}", value)
                    }
                } else {
                    format!("{:?}", value)
                }
            }
            ReflectRef::Enum(e) => {
                // Pour les enums, formater comme "Variant(value)"
                let variant_name = e.variant_name();
                if e.field_len() == 1 {
                    if let Some(field) = e.field_at(0) {
                        // Convertir PartialReflect en Reflect si possible
                        if let Some(reflect_val) = field.try_as_reflect() {
                            format!("{}", Self::reflect_to_string(reflect_val))
                        } else {
                            variant_name.to_string()
                        }
                    } else {
                        variant_name.to_string()
                    }
                } else {
                    variant_name.to_string()
                }
            }
            _ => {
                // Fallback: utiliser Debug et échapper
                let debug_str = format!("{:?}", value);
                escape(&debug_str).to_string()
            }
        }
    }

    /// Sérialise une structure Reflect en XML simple.
    fn serialize_struct_to_xml(s: &dyn bevy_reflect::Struct) -> String {
        use bevy_reflect::TypeInfo;
        use std::fmt::Write;

        let mut xml = String::new();

        // Commencer par ouvrir la balise avec le nom du type
        let type_name = s
            .get_represented_type_info()
            .and_then(|ti| {
                if let TypeInfo::Struct(si) = ti {
                    Some(si.type_path_table().short_path())
                } else {
                    None
                }
            })
            .unwrap_or("struct");

        let _ = write!(&mut xml, "<{}>", type_name);

        // Ajouter chaque champ
        for i in 0..s.field_len() {
            if let Some(field_name) = s.name_at(i) {
                if let Some(field_value) = s.field_at(i) {
                    // Convertir PartialReflect en Reflect si possible
                    if let Some(reflect_val) = field_value.try_as_reflect() {
                        let value_str = Self::reflect_to_string(reflect_val);
                        let _ = write!(&mut xml, "<{}>{}</{}>", field_name, value_str, field_name);
                    }
                }
            }
        }

        let _ = write!(&mut xml, "</{}>", type_name);

        xml
    }

    /// Démarre le notifier périodique.
    ///
    /// # Arguments
    ///
    /// * `interval` - Intervalle entre les notifications
    ///
    /// # Returns
    ///
    /// Un handle vers la tâche tokio du notifier.
    pub fn start_notifier(&self, interval: Duration) -> tokio::task::JoinHandle<()> {
        let instance = self.clone();

        tokio::spawn(async move {
            let mut ticker = time::interval(interval);
            info!("✅ Starting notifier every {:?}", interval);

            loop {
                ticker.tick().await;
                instance.notify_subscribers().await;
            }
        })
    }
}

/// Handler Axum pour les événements (SUBSCRIBE/UNSUBSCRIBE).
///
/// Gère les requêtes HTTP SUBSCRIBE et UNSUBSCRIBE selon la spécification
/// UPnP Device Architecture.
///
/// # Opérations supportées
///
/// - `SUBSCRIBE` sans SID : Nouvelle souscription
/// - `SUBSCRIBE` avec SID : Renouvellement d'abonnement
/// - `UNSUBSCRIBE` : Annulation d'abonnement
///
/// # Arguments
///
/// * `instance` - L'instance du service
/// * `headers` - En-têtes HTTP de la requête
/// * `req` - La requête HTTP complète
///
/// # Returns
///
/// Une réponse HTTP avec le SID et le timeout pour SUBSCRIBE,
/// ou une simple confirmation pour UNSUBSCRIBE.
async fn event_sub_handler(
    State(instance): State<ServiceInstance>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Response {
    info!("📡 Event Subscription request for {}", instance.get_name());

    let method = req.method().as_str();
    let sid = headers
        .get("SID")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let timeout = headers
        .get("Timeout")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let callback = headers
        .get("Callback")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    match method {
        METHOD_SUBSCRIBE => {
            let (response_sid, response_timeout) = if sid.is_empty() {
                // Nouvelle souscription
                let new_sid = format!("uuid:{}", uuid::Uuid::new_v4());
                if !callback.is_empty() {
                    instance
                        .add_subscriber(new_sid.clone(), callback.to_string())
                        .await;
                }
                let timeout_val = if timeout.is_empty() {
                    "Second-1800"
                } else {
                    timeout
                };
                info!(
                    "🔒 New subscription: SID={}, Callback={}, Timeout={}",
                    new_sid, callback, timeout_val
                );

                let sid_clone = new_sid.clone();
                let instance_clone = instance.clone();
                tokio::spawn(async move {
                    instance_clone.send_initial_event(sid_clone).await;
                });

                (new_sid, timeout_val.to_string())
            } else {
                // Renouvellement
                instance.renew_subscriber(sid, timeout).await;
                info!("♻️ Renew subscription: SID={}, Timeout={}", sid, timeout);
                (sid.to_string(), timeout.to_string())
            };

            (
                StatusCode::OK,
                [
                    (
                        axum::http::header::HeaderName::from_static("sid"),
                        axum::http::HeaderValue::from_str(&response_sid).unwrap(),
                    ),
                    (
                        axum::http::header::HeaderName::from_static("timeout"),
                        axum::http::HeaderValue::from_str(&response_timeout).unwrap(),
                    ),
                ],
            )
                .into_response()
        }
        METHOD_UNSUBSCRIBE => {
            if !sid.is_empty() {
                instance.remove_subscriber(sid).await;
                info!("❌ Unsubscribe SID={}", sid);
            }
            StatusCode::OK.into_response()
        }
        _ => {
            warn!("Unsupported EventSub method: {}", method);
            StatusCode::METHOD_NOT_ALLOWED.into_response()
        }
    }
}

/// Handler Axum pour le contrôle SOAP.
///
/// Gère les requêtes de contrôle SOAP pour invoquer des actions sur le service.
///
/// # Arguments
///
/// * `instance` - L'instance du service (Arc-wrapped)
/// * `body` - Corps de la requête SOAP
///
/// # Returns
///
/// Une réponse SOAP avec le résultat de l'action, ou un SOAP fault en cas d'erreur.
///
/// # Erreurs
///
/// Retourne un SOAP fault dans les cas suivants :
/// - Parsing SOAP invalide
/// - Action non trouvée
/// - Arguments invalides
/// - Échec de l'exécution de l'action
async fn control_handler(State(instance): State<Arc<ServiceInstance>>, body: String) -> Response {
    use crate::{
        UpnpTypedInstance,
        soap::{build_soap_fault, build_soap_response, error_codes, parse_soap_action},
        variable_types::{StateValue, UpnpVarType},
    };
    use std::collections::HashMap;
    use tracing::debug;

    info!("📡 Control request for {}", instance.get_name());

    // Parser le SOAP pour extraire l'action et ses arguments
    let soap_action = match parse_soap_action(body.as_bytes()) {
        Ok(action) => action,
        Err(e) => {
            error!("❌ Failed to parse SOAP: {:?}", e);
            let fault_xml = build_soap_fault(
                "s:Client",
                "Invalid SOAP request",
                Some(error_codes::INVALID_ACTION),
                Some("The SOAP request could not be parsed")
            ).unwrap_or_else(|_| String::from("<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><s:Fault><faultcode>s:Server</faultcode><faultstring>Internal Error</faultstring></s:Fault></s:Body></s:Envelope>"));
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(
                    axum::http::header::CONTENT_TYPE,
                    "text/xml; charset=\"utf-8\"",
                )],
                fault_xml,
            )
                .into_response();
        }
    };

    debug!("🎬 Received SOAP action: {}", soap_action.name);
    debug!("🎬 SOAP arguments: {:?}", soap_action.args);

    // Trouver l'action correspondante dans l'instance
    let action_instance = match instance.action(&soap_action.name) {
        Some(action_inst) => action_inst,
        None => {
            error!("❌ Action not found: {}", soap_action.name);
            let fault_xml = build_soap_fault(
                "s:Client",
                "Invalid Action",
                Some(error_codes::INVALID_ACTION),
                Some(&format!("Action '{}' not found", soap_action.name))
            ).unwrap_or_else(|_| String::from("<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><s:Fault><faultcode>s:Server</faultcode><faultstring>Internal Error</faultstring></s:Fault></s:Body></s:Envelope>"));
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(
                    axum::http::header::CONTENT_TYPE,
                    "text/xml; charset=\"utf-8\"",
                )],
                fault_xml,
            )
                .into_response();
        }
    };

    // Convertir les arguments SOAP (String) en StateValue
    let mut soap_values = HashMap::new();
    for (arg_name, arg_value) in soap_action.args {
        debug!("🔍 Processing SOAP arg: {} = '{}'", arg_name, arg_value);
        // Trouver l'argument correspondant pour obtenir son type
        if let Some(arg_inst) = action_instance.argument(&arg_name) {
            if let Some(var_inst) = arg_inst.get_variable_instance() {
                let var_model = var_inst.as_ref().get_model();
                // Parser la valeur selon le type de la variable
                match StateValue::from_string(&arg_value, &var_model.as_state_var_type()) {
                    Ok(value) => {
                        debug!("✅ Parsed {} = {:?}", arg_name, value);
                        soap_values.insert(arg_name, value);
                    }
                    Err(e) => {
                        error!("❌ Failed to parse argument '{}': {:?}", arg_name, e);
                        let fault_xml = build_soap_fault(
                            "s:Client",
                            "Invalid Arguments",
                            Some(error_codes::ARGUMENT_VALUE_INVALID),
                            Some(&format!("Invalid value for argument '{}'", arg_name))
                        ).unwrap_or_else(|_| String::from("<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><s:Fault><faultcode>s:Server</faultcode><faultstring>Internal Error</faultstring></s:Fault></s:Body></s:Envelope>"));
                        return (
                            StatusCode::BAD_REQUEST,
                            [(
                                axum::http::header::CONTENT_TYPE,
                                "text/xml; charset=\"utf-8\"",
                            )],
                            fault_xml,
                        )
                            .into_response();
                    }
                }
            }
        }
    }

    let soap_values = Arc::new(soap_values);

    // Exécuter l'action
    let action_instance_for_run = Arc::clone(&action_instance);

    match action_instance_for_run.run(soap_values).await {
        Ok(output_data) => {
            // Convertir ActionData (Reflect) → HashMap<String, String> pour SOAP
            let mut soap_values: Vec<(String, String)> = Vec::new();

            for arg_inst in action_instance.arguments_set().all() {
                let arg_model = arg_inst.as_ref().get_model();
                if arg_model.is_out() {
                    if let Some(reflect_value) = output_data.get(arg_inst.get_name()) {
                        let soap_string =
                            ServiceInstance::reflect_to_string(reflect_value.as_ref());
                        soap_values.push((arg_inst.get_name().to_string(), soap_string));
                    }
                }
            }

            // Construire la réponse SOAP
            let response_xml = build_soap_response(
                &instance.service_type(),
                &soap_action.name,
                soap_values
            ).unwrap_or_else(|_| {
                build_soap_fault(
                    "s:Server",
                    "Action Failed",
                    Some(error_codes::ACTION_FAILED),
                    Some("Failed to build SOAP response")
                ).unwrap_or_else(|_| String::from("<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><s:Fault><faultcode>s:Server</faultcode><faultstring>Internal Error</faultstring></s:Fault></s:Body></s:Envelope>"))
            });

            (
                StatusCode::OK,
                [(
                    axum::http::header::CONTENT_TYPE,
                    "text/xml; charset=\"utf-8\"",
                )],
                response_xml,
            )
                .into_response()
        }
        Err(e) => {
            error!("❌ Action execution failed: {:?}", e);
            let fault_xml = build_soap_fault(
                "s:Server",
                "Action Failed",
                Some(error_codes::ACTION_FAILED),
                Some(&format!("Action execution failed: {:?}", e))
            ).unwrap_or_else(|_| String::from("<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><s:Fault><faultcode>s:Server</faultcode><faultstring>Internal Error</faultstring></s:Fault></s:Body></s:Envelope>"));
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(
                    axum::http::header::CONTENT_TYPE,
                    "text/xml; charset=\"utf-8\"",
                )],
                fault_xml,
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::Service;
    use bevy_reflect::Reflect;

    #[test]
    fn test_reflect_to_string_primitives() {
        // Test des types primitifs
        assert_eq!(ServiceInstance::reflect_to_string(&42i32), "42");
        assert_eq!(ServiceInstance::reflect_to_string(&3.14f64), "3.14");
        assert_eq!(ServiceInstance::reflect_to_string(&true), "1");
        assert_eq!(ServiceInstance::reflect_to_string(&false), "0");
        assert_eq!(ServiceInstance::reflect_to_string(&'a'), "a");
    }

    #[test]
    fn test_reflect_to_string_xml_escaping() {
        // Test de l'échappement XML
        let test_str = "Test <tag> & \"quotes\"".to_string();
        let result = ServiceInstance::reflect_to_string(&test_str);

        // Vérifier que les caractères sont échappés
        assert!(result.contains("&lt;"));
        assert!(result.contains("&gt;"));
        assert!(result.contains("&amp;"));
        assert!(result.contains("&quot;"));
    }

    #[test]
    fn test_reflect_to_string_struct() {
        #[derive(Debug, Clone, Reflect)]
        struct TestStruct {
            name: String,
            value: i32,
        }

        let test = TestStruct {
            name: "Test".to_string(),
            value: 42,
        };

        let result = ServiceInstance::reflect_to_string(&test);

        // Vérifier que c'est du XML
        assert!(result.starts_with("<"));
        assert!(result.ends_with(">"));
        assert!(result.contains("name"));
        assert!(result.contains("value"));
        assert!(result.contains("Test"));
        assert!(result.contains("42"));

        println!("Serialized struct: {}", result);
    }

    #[test]
    fn test_reflect_to_string_nested_struct() {
        #[derive(Debug, Clone, Reflect)]
        struct Address {
            street: String,
            city: String,
        }

        #[derive(Debug, Clone, Reflect)]
        struct Person {
            name: String,
            age: u32,
            address: Address,
        }

        let person = Person {
            name: "John <Doe>".to_string(), // Test XML escaping
            age: 30,
            address: Address {
                street: "123 Main St & Ave".to_string(),
                city: "Springfield".to_string(),
            },
        };

        let result = ServiceInstance::reflect_to_string(&person);

        // Vérifier la structure XML
        assert!(result.contains("<Person>"));
        assert!(result.contains("</Person>"));
        assert!(result.contains("name"));
        assert!(result.contains("age"));
        assert!(result.contains("address"));

        // Vérifier l'échappement XML dans les valeurs imbriquées
        assert!(result.contains("&lt;"));
        assert!(result.contains("&gt;"));
        assert!(result.contains("&amp;"));

        println!("Nested struct XML: {}", result);
    }

    #[test]
    fn test_service_instance_creation() {
        let service = Service::new("AVTransport".to_string());
        let instance = ServiceInstance::new(&service);

        assert_eq!(instance.get_name(), "AVTransport");
        assert_eq!(instance.identifier(), "AVTransport");
    }

    #[test]
    fn test_service_urls() {
        let service = Service::new("AVTransport".to_string());
        let instance = ServiceInstance::new(&service);

        assert_eq!(instance.route(), "/service/AVTransport");
        assert_eq!(instance.control_route(), "/service/AVTransport/control");
        assert_eq!(instance.event_route(), "/service/AVTransport/event");
        assert_eq!(instance.scpd_route(), "/service/AVTransport/desc.xml");
    }

    #[test]
    fn test_service_type() {
        let mut service = Service::new("AVTransport".to_string());
        service.set_version(2).unwrap();
        let instance = ServiceInstance::new(&service);

        assert_eq!(
            instance.service_type(),
            "urn:schemas-upnp-org:service:AVTransport:2"
        );
    }
}
