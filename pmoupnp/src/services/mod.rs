mod errors;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use axum::{
    extract::{Request, State},
    response::{Html, IntoResponse, Response},
    http::{StatusCode, HeaderMap},
    body::Body,
};
use std::sync::RwLock;
use tokio::time;
use tracing::{info, warn, debug, error};

use crate::actions::{Action, ActionSet, ActionInstance, ActionInstanceSet};
use crate::state_variables::{StateVariable, StateVariableSet, StateVarInstance, StateVarInstanceSet};

pub use errors::ServiceError;

#[derive(Debug, Clone)]
pub struct UpnpObjectType {
    name: String,
    object_type: String,
}

impl UpnpObjectType {
    pub fn new(name: String, object_type: String) -> Self {
        Self { name, object_type }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn object_type(&self) -> &str {
        &self.object_type
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }
}

#[derive(Debug, Clone)]
pub struct Service {
    object: UpnpObjectType,
    identifier: String,
    version: u32,
    actions: ActionSet,
    state_table: StateVariableSet,
}

impl Service {
    pub fn new(name: String) -> Self {
        Self {
            object: UpnpObjectType::new(name.clone(), "Service".to_string()),
            identifier: name,
            version: 1,
            state_table: StateVariableSet::new(),
            actions: ActionSet::new(),
        }
    }

    pub fn name(&self) -> &str {
        self.object.name()
    }

    pub fn type_id(&self) -> &str {
        self.object.object_type()
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    pub fn set_identifier(&mut self, id: String) {
        self.identifier = id;
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn set_version(&mut self, version: u32) -> Result<(), String> {
        if version < 1 {
            return Err("version must be greater than or equal to 1".to_string());
        }
        self.version = version; 
        Ok(())
    }

    pub fn add_variable(&mut self, sv: Arc<StateVariable>)  {
        self.state_table.insert(sv);
    }

    pub fn contains_variable(&self, sv: Arc<StateVariable>) -> bool {
        self.state_table.contains(sv).await
    }

    pub fn variables(&self) -> impl Iterator<Item = &StateVariable> {
        self.state_table.iter()
    }

    pub fn add_action(&mut self, action: Action) -> Result<(), ServiceError> {
        self.actions.insert(action)
    }

    pub fn new_instance(&self) -> ServiceInstance {
        // 1️⃣ D'abord créer les StateVarInstance
        let mut statevariables = StateVarInstanceSet::new();
        for v in self.state_table.all() {
            statevariables.insert(v.new_instance());
        }

        // 2️⃣ Ensuite créer les ActionInstance en vérifiant les variables
        let mut actions = ActionInstanceSet::new();
        for a in self.actions.all() {
            // Vérifier que toutes les variables d'état référencées existent
            let mut missing_vars = Vec::new();
            
            for arg in a.arguments().iter() {
                let related_var_name = arg.state_variable().get_name();
                if !statevariables.contains(related_var_name) {
                    missing_vars.push(related_var_name.to_string());
                }
            }
            
            if !missing_vars.is_empty() {
                error!(
                    "❌ Action '{}' references missing state variables: {:?}",
                    a.get_name(),
                    missing_vars
                );
                continue; // Skip cette action
            }
            
            if let Err(e) = actions.insert(a.new_instance()) {
                error!("❌ Failed to insert action '{}': {:?}", a.get_name(), e);
            }
        }

        ServiceInstance {
            name: self.name().to_string(),
            identifier: self.identifier.clone(),
            version: self.version,
            device: None,
            statevariables,
            actions,
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            changed_buffer: Arc::new(Mutex::new(HashMap::new())),
            seqid: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceInstance {
    name: String,
    identifier: String,
    version: u32,
    device: Option<Arc<DeviceInstance>>,
    statevariables: StateVarInstanceSet,
    actions: ActionInstanceSet,
    subscribers: Arc<RwLock<HashMap<String, String>>>, // SID → Callback URL
    changed_buffer: Arc<Mutex<HashMap<String, String>>>, // Simplifié pour l'exemple
    seqid: Arc<Mutex<HashMap<String, u32>>>,
}

pub const METHOD_SUBSCRIBE: &str = "SUBSCRIBE";
pub const METHOD_UNSUBSCRIBE: &str = "UNSUBSCRIBE";

impl ServiceInstance {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn type_id(&self) -> &str {
        "ServiceInstance"
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    pub fn service_type(&self) -> String {
        format!("urn:schemas-upnp-org:service:{}:{}", self.name, self.version)
    }

    pub fn service_id(&self) -> String {
        format!("urn:upnp-org:serviceId:{}", self.identifier)
    }

    pub fn base_route(&self) -> String {
        match &self.device {
            Some(device) => format!("{}/service/{}", device.base_route(), self.name),
            None => format!("/service/{}", self.name),
        }
    }

    pub fn control_url(&self) -> String {
        format!("{}/control", self.base_route())
    }

    pub fn event_sub_url(&self) -> String {
        format!("{}/event", self.base_route())
    }

    pub fn scpd_url(&self) -> String {
        format!("{}/desc.xml", self.base_route())
    }

    pub fn usn(&self) -> String {
        match &self.device {
            Some(device) => format!("uuid:{}::urn:{}", device.udn(), self.service_type()),
            None => format!("uuid::urn:{}", self.service_type()),
        }
    }

    pub fn statevariables(&self) -> &StateVarInstanceSet {
        &self.statevariables
    }

    pub fn actions(&self) -> &ActionInstanceSet {
        &self.actions
    }

    /// Enregistre les routes UPnP dans le serveur Axum
    pub async fn register_urls(&self, server: &mut crate::server::Server) -> Result<(), String> {
        info!(
            "✅ Service description for {}:{} available at : {}{}",
            self.device.as_ref().map(|d| d.name()).unwrap_or("unknown"),
            self.name(),
            self.device.as_ref().map(|d| d.server_base_url()).unwrap_or(""),
            self.scpd_url(),
        );

        // Handler pour la description SCPD
        let instance_scpd = self.clone();
        server.add_handler(&self.scpd_url(), move || {
            let instance = instance_scpd.clone();
            async move { instance.scpd_handler().await }
        }).await;

        // Handler pour le contrôle
        let instance_control = self.clone();
        server.add_post_handler_with_state(
            &self.control_url(),
            control_handler,
            instance_control,
        ).await;

        // Handler pour les événements
        let instance_event = self.clone();
        server.add_handler_with_state(
            &self.event_sub_url(),
            event_sub_handler,
            instance_event,
        ).await;

        Ok(())
    }

    /// Génère l'élément XML SCPD
    pub fn scpd_element(&self) -> xmltree::Element {
        let mut elem = xmltree::Element::new("scpd");
        elem.attributes.insert(
            "xmlns".to_string(),
            "urn:schemas-upnp-org:service-1-0".to_string(),
        );

        // Version spec
        let mut spec = xmltree::Element::new("specVersion");
        let mut major = xmltree::Element::new("major");
        major.children.push(xmltree::XMLNode::Text("1".to_string()));
        spec.children.push(xmltree::XMLNode::Element(major));
        
        let mut minor = xmltree::Element::new("minor");
        minor.children.push(xmltree::XMLNode::Text("0".to_string()));
        spec.children.push(xmltree::XMLNode::Element(minor));
        
        elem.children.push(xmltree::XMLNode::Element(spec));

        // Actions
        if !self.actions.is_empty() {
            elem.children.push(xmltree::XMLNode::Element(
                self.actions.to_xml_element()
            ));
        }

        // State variables
        if !self.statevariables.is_empty() {
            elem.children.push(xmltree::XMLNode::Element(
                self.statevariables.to_xml_element()
            ));
        }

        elem
    }

    /// Handler pour le SCPD
    async fn scpd_handler(&self) -> Response {
        let elem = self.scpd_element();
        
        let mut xml_output = Vec::new();
        if let Err(e) = elem.write(&mut xml_output) {
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

    /// Génère l'élément XML du service
    pub fn to_xml_element(&self) -> xmltree::Element {
        let mut elem = xmltree::Element::new("service");

        let mut service_type = xmltree::Element::new("serviceType");
        service_type.children.push(xmltree::XMLNode::Text(self.service_type()));
        elem.children.push(xmltree::XMLNode::Element(service_type));

        let mut service_id = xmltree::Element::new("serviceId");
        service_id.children.push(xmltree::XMLNode::Text(self.service_id()));
        elem.children.push(xmltree::XMLNode::Element(service_id));

        let mut scpd_url = xmltree::Element::new("SCPDURL");
        scpd_url.children.push(xmltree::XMLNode::Text(self.scpd_url()));
        elem.children.push(xmltree::XMLNode::Element(scpd_url));

        let mut control_url = xmltree::Element::new("controlURL");
        control_url.children.push(xmltree::XMLNode::Text(self.control_url()));
        elem.children.push(xmltree::XMLNode::Element(control_url));

        let mut event_sub_url = xmltree::Element::new("eventSubURL");
        event_sub_url.children.push(xmltree::XMLNode::Text(self.event_sub_url()));
        elem.children.push(xmltree::XMLNode::Element(event_sub_url));

        elem
    }

    pub async fn add_subscriber(&self, sid: String, callback: String) {
        let mut subscribers = self.subscribers.write().await;
        subscribers.insert(sid, callback);
    }

    pub async fn renew_subscriber(&self, sid: &str, timeout: &str) {
        info!("♻️ Renewed SID {} for timeout {}", sid, timeout);
    }

    pub async fn remove_subscriber(&self, sid: &str) {
        let mut subscribers = self.subscribers.write().await;
        subscribers.remove(sid);
    }

    pub async fn send_initial_event(&self, sid: String) {
        let callback = {
            let subscribers = self.subscribers.read().await;
            subscribers.get(&sid).cloned()
        };

        if let Some(callback) = callback {
            let mut changed = HashMap::new();
            for sv in self.statevariables.iter() {
                if sv.is_sending_events() {
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
                    .body(body.clone())
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

    pub fn event_to_be_sent(&self, name: String, value: String) {
        let mut buffer = self.changed_buffer.lock().unwrap();
        buffer.insert(name, value);
    }

    fn next_seq(&self, sid: &str) -> String {
        let mut seqid = self.seqid.lock().unwrap();
        let counter = seqid.entry(sid.to_string()).or_insert(0);
        *counter += 1;
        counter.to_string()
    }

    pub async fn notify_subscribers(&self) {
        let subscribers_copy = {
            let subscribers = self.subscribers.read().await;
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

// Handler Axum pour les événements (SUBSCRIBE/UNSUBSCRIBE)
async fn event_sub_handler(
    State(instance): State<ServiceInstance>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Response {
    info!("📡 Event Subscription request for {}", instance.name());

    let method = req.method().as_str();
    let sid = headers.get("SID").and_then(|v| v.to_str().ok()).unwrap_or("");
    let timeout = headers.get("Timeout").and_then(|v| v.to_str().ok()).unwrap_or("");
    let callback = headers.get("Callback").and_then(|v| v.to_str().ok()).unwrap_or("");

    match method {
        METHOD_SUBSCRIBE => {
            let (response_sid, response_timeout) = if sid.is_empty() {
                // Nouvelle subscription
                let new_sid = format!("uuid:{}", uuid::Uuid::new_v4());
                if !callback.is_empty() {
                    instance.add_subscriber(new_sid.clone(), callback.to_string()).await;
                }
                let timeout_val = if timeout.is_empty() {
                    "Second-1800"
                } else {
                    timeout
                };
                info!("🔔 New subscription: SID={}, Callback={}, Timeout={}", new_sid, callback, timeout_val);
                
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
                    (axum::http::header::HeaderName::from_static("sid"), response_sid.parse().unwrap()),
                    (axum::http::header::HeaderName::from_static("timeout"), response_timeout.parse().unwrap()),
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
}

// Handler Axum pour le contrôle SOAP
async fn control_handler(
    State(instance): State<ServiceInstance>,
    body: String,
) -> Response {
    info!("📡 Control request for {}", instance.name());

    // TODO: Parser le SOAP et appeler l'action correspondante
    // Pour l'instant, réponse minimale
    
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
}

// Type placeholder pour DeviceInstance
#[derive(Debug, Clone)]
pub struct DeviceInstance {
    name: String,
    udn: String,
}

impl DeviceInstance {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn base_route(&self) -> String {
        format!("/device/{}", self.name)
    }
    
    pub fn udn(&self) -> &str {
        &self.udn
    }

    pub fn server_base_url(&self) -> String {
        "http://localhost:8080".to_string()
    }
}