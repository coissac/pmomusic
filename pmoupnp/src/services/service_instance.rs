//! Implémentation de ServiceInstance.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
    pin::Pin,
    future::Future,
};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    body::Body,
};
use tokio::time;
use tracing::{info, warn, error};
use xmltree::{Element, XMLNode, EmitterConfig};

use crate::{
    services::{Service, ServiceError},
    actions::{ActionInstance, ActionInstanceSet},
    state_variables::{StateVarInstance, StateVarInstanceSet, UpnpVariable},
    devices::DeviceInstance,
    UpnpObject, UpnpInstance, UpnpTyped, UpnpTypedInstance, UpnpObjectType,
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
/// ```rust,no_run
/// # use pmoupnp::services::Service;
/// # use pmoupnp::server::Server;
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
    
    /// Buffer des changements en attente de notification
    changed_buffer: Arc<Mutex<HashMap<String, String>>>,
    
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
        service_type.children.push(XMLNode::Text(self.service_type()));
        elem.children.push(XMLNode::Element(service_type));

        let mut service_id = Element::new("serviceId");
        service_id.children.push(XMLNode::Text(self.service_id()));
        elem.children.push(XMLNode::Element(service_id));

        let mut scpd_url = Element::new("SCPDURL");
        scpd_url.children.push(XMLNode::Text(self.scpd_route()));
        elem.children.push(XMLNode::Element(scpd_url));

        let mut control_url = Element::new("controlURL");
        control_url.children.push(XMLNode::Text(self.control_route()));
        elem.children.push(XMLNode::Element(control_url));

        let mut event_sub_url = Element::new("eventSubURL");
        event_sub_url.children.push(XMLNode::Text(self.event_route()));
        elem.children.push(XMLNode::Element(event_sub_url));

        elem
    }
}

impl ServiceInstance {
    /// Retourne l'identifiant du service.
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Retourne le type de service UPnP.
    ///
    /// Format: `urn:schemas-upnp-org:service:{name}:{version}`
    pub fn service_type(&self) -> String {
        self.model.service_type()
    }

    /// Retourne l'ID de service UPnP.
    ///
    /// Format: `urn:upnp-org:serviceId:{identifier}`
    pub fn service_id(&self) -> String {
        format!("urn:upnp-org:serviceId:{}", self.identifier)
    }

    /// Raccourci pour obtenir une variable d'état par nom
    pub fn get_variable(&self, name: &str) -> Option<Arc<StateVarInstance>> {
        self.statevariables.get_by_name(name)
    }
    
    /// Raccourci pour obtenir une action par nom
    pub fn get_action(&self, name: &str) -> Option<Arc<ActionInstance>> {
        self.actions.get_by_name(name)
    }

    /// Définit le device parent pour ce service.
    ///
    /// Cette méthode doit être appelée après la création du service instance
    /// pour établir la relation avec le device parent.
    pub fn set_device(&self, device: Arc<DeviceInstance>) {
        let mut dev = self.device.write().unwrap();
        *dev = Some(device);
    }

    /// Retourne la route du service (chemin relatif).
    pub fn route(&self) -> String {
        let device = self.device.read().unwrap();
        match device.as_ref() {
            Some(device) => format!("{}/service/{}", device.route(), self.get_name()),
            None => format!("/service/{}", self.get_name()),
        }
    }

    /// Retourne la route de contrôle SOAP.
    pub fn control_route(&self) -> String {
        format!("{}/control", self.route())
    }

    /// Retourne la route de souscription aux événements.
    pub fn event_route(&self) -> String {
        format!("{}/event", self.route())
    }

    /// Retourne la route de la description SCPD.
    pub fn scpd_route(&self) -> String {
        format!("{}/desc.xml", self.route())
    }

    /// Retourne l'USN (Unique Service Name).
    pub fn usn(&self) -> String {
        let device = self.device.read().unwrap();
        match device.as_ref() {
            Some(device) => format!("uuid:{}::urn:{}", device.udn(), self.service_type()),
            None => format!("uuid::urn:{}", self.service_type()),
        }
    }

    /// Retourne les variables d'état.
    pub fn statevariables(&self) -> &StateVarInstanceSet {
        &self.statevariables
    }

    /// Retourne les actions.
    pub fn actions(&self) -> &ActionInstanceSet {
        &self.actions
    }

    /// Enregistre les routes UPnP dans le serveur.
    ///
    /// # Errors
    ///
    /// Retourne une erreur si l'enregistrement des routes échoue.
    pub async fn register_urls<S: crate::UpnpServer + ?Sized>(&self, server: &mut S) -> Result<(), ServiceError> {
        let device = self.device.read().unwrap();
        let device_name = device.as_ref().map(|d| d.get_name().clone()).unwrap_or_else(|| "unknown".to_string());
        let server_url = device.as_ref().map(|d| d.base_url().to_string()).unwrap_or_default();
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
        server.add_handler(&self.scpd_route(), move || {
            let instance = instance_scpd.clone();
            async move { instance.scpd_handler().await }
        }).await;

        // Handler control
        let instance_control = self.clone();
        server.add_post_handler_with_state(
            &self.control_route(),
            control_handler,
            instance_control,
        ).await;

        // Handler événements
        let instance_event = self.clone();
        server.add_handler_with_state(
            &self.event_route(),
            event_sub_handler,
            instance_event,
        ).await;

        Ok(())
    }

    /// Génère l'élément XML SCPD.
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

        // actionList
        if !self.actions.all().is_empty() {
            elem.children.push(XMLNode::Element(
                self.actions.to_xml_element()
            ));
        }

        // serviceStateTable
        if !self.statevariables.all().is_empty() {
            elem.children.push(XMLNode::Element(
                self.statevariables.to_xml_element()
            ));
        }

        elem
    }

    /// Handler pour la description SCPD.
    async fn scpd_handler(&self) -> Response {
        let elem = self.scpd_element();
        
        let config = EmitterConfig::new()
            .perform_indent(true)
            .indent_string("  ");
        
        let mut xml_output = Vec::new();
        if let Err(e) = elem.write_with_config(&mut xml_output, config) {
            error!("Failed to serialize SCPD XML: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        let xml = String::from_utf8_lossy(&xml_output).to_string();

        (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/xml; charset=\"utf-8\"")],
            xml,
        ).into_response()
    }

    /// Ajoute un abonné aux événements.
    pub async fn add_subscriber(&self, sid: String, callback: String) {
        let mut subscribers = self.subscribers.write().unwrap();
        subscribers.insert(sid, callback);
    }

    /// Renouvelle un abonnement.
    pub async fn renew_subscriber(&self, sid: &str, timeout: &str) {
        info!("♻️ Renewed SID {} for timeout {}", sid, timeout);
    }

    /// Supprime un abonné.
    pub async fn remove_subscriber(&self, sid: &str) {
        let mut subscribers = self.subscribers.write().unwrap();
        subscribers.remove(sid);
    }

    /// Envoie l'événement initial à un nouvel abonné.
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
                
                let mut body = r#"<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">"#.to_string();
                for (name, val) in changed {
                    body.push_str(&format!("<e:property><{0}>{1}</{0}></e:property>", name, val));
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
                        info!("✅ Initial event sent to {}, status={}", callback, resp.status());
                    }
                    Err(e) => {
                        error!("Failed to send initial event to {}: {}", callback, e);
                    }
                }
            });
        }
    }

    /// Marque un changement à notifier.
    pub fn event_to_be_sent(&self, name: String, value: String) {
        let mut buffer = self.changed_buffer.lock().unwrap();
        buffer.insert(name, value);
    }

    /// Récupère le prochain numéro de séquence pour un abonné.
    fn next_seq(&self, sid: &str) -> String {
        let mut seqid = self.seqid.lock().unwrap();
        let counter = seqid.entry(sid.to_string()).or_insert(0);
        *counter += 1;
        counter.to_string()
    }

    /// Notifie tous les abonnés des changements.
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

                let mut body = r#"<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">"#.to_string();
                for (name, val) in changed_clone {
                    body.push_str(&format!("<e:property><{0}>{1}</{0}></e:property>", name, val));
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
fn event_sub_handler(
    State(instance): State<ServiceInstance>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Pin<Box<dyn Future<Output = Response> + Send>> {
    Box::pin(async move {
    info!("📡 Event Subscription request for {}", instance.get_name());

    let method = req.method().as_str();
    let sid = headers.get("SID").and_then(|v| v.to_str().ok()).unwrap_or("");
    let timeout = headers.get("Timeout").and_then(|v| v.to_str().ok()).unwrap_or("");
    let callback = headers.get("Callback").and_then(|v| v.to_str().ok()).unwrap_or("");

    match method {
        METHOD_SUBSCRIBE => {
            let (response_sid, response_timeout) = if sid.is_empty() {
                // Nouvelle souscription
                let new_sid = format!("uuid:{}", uuid::Uuid::new_v4());
                if !callback.is_empty() {
                    instance.add_subscriber(new_sid.clone(), callback.to_string()).await;
                }
                let timeout_val = if timeout.is_empty() {
                    "Second-1800"
                } else {
                    timeout
                };
                info!("🔒 New subscription: SID={}, Callback={}, Timeout={}", new_sid, callback, timeout_val);
                
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
                        axum::http::HeaderValue::from_str(&response_sid).unwrap()
                    ),
                    (
                        axum::http::header::HeaderName::from_static("timeout"), 
                        axum::http::HeaderValue::from_str(&response_timeout).unwrap()
                    ),
                ],
            ).into_response()
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
    })
}

/// Handler Axum pour le contrôle SOAP.
fn control_handler(
    State(instance): State<ServiceInstance>,
    _body: String,
) -> Pin<Box<dyn Future<Output = Response> + Send>> {
    Box::pin(async move {
    info!("📡 Control request for {}", instance.get_name());

    // TODO: Parser le SOAP et appeler l'action correspondante
    
    let response_xml = format!(
        r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" 
            s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
    <s:Body>
        <u:Response xmlns:u="{}">
        </u:Response>
    </s:Body>
</s:Envelope>"#,
        instance.service_type()
    );

    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/xml; charset=\"utf-8\"")],
        response_xml,
    ).into_response()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::Service;

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