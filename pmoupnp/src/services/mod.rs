//! # Module Services - Gestion des services UPnP
//!
//! Ce module implémente les services UPnP selon la spécification UPnP Device Architecture.
//! Un service UPnP contient des actions (méthodes appelables) et des variables d'état
//! (propriétés observables).
//!
//! ## Architecture
//!
//! - [`Service`] : Modèle définissant la structure d'un service
//! - [`ServiceInstance`] : Instance concrète d'un service attachée à un device
//!
//! ## Fonctionnalités
//!
//! - ✅ Actions UPnP avec arguments typés
//! - ✅ Variables d'état avec notifications d'événements
//! - ✅ Génération SCPD (Service Control Protocol Description)
//! - ✅ Endpoints SOAP pour le contrôle
//! - ✅ Gestion des abonnements aux événements (SUBSCRIBE/UNSUBSCRIBE)
//! - ✅ Notifications automatiques des changements d'état
//!
//! ## Examples
//!
//! ```rust
//! use pmoupnp::services::Service;
//! use pmoupnp::state_variables::StateVariable;
//! use pmoupnp::variable_types::StateVarType;
//! use std::sync::Arc;
//!
//! // Créer un service
//! let mut service = Service::new("AVTransport".to_string());
//! service.set_version(1).unwrap();
//!
//! // Ajouter une variable d'état
//! let transport_state = Arc::new(
//!     StateVariable::new(StateVarType::String, "TransportState".to_string())
//! );
//! service.add_variable(transport_state);
//!
//! // Créer une instance
//! let instance = service.create_instance();
//! ```

mod errors;
mod macros;
mod service_instance;
mod service_methods;

use std::sync::Arc;

pub use errors::ServiceError;
pub use service_instance::ServiceInstance;
use xmltree::{Element, EmitterConfig, XMLNode};

use crate::{actions::ActionSet, state_variables::StateVariableSet, UpnpObject, UpnpObjectType};

/// Service UPnP (modèle).
///
/// Représente la définition d'un service UPnP avec ses actions et variables d'état.
/// Un service est attaché à un device et expose des fonctionnalités via SOAP.
///
/// # Structure
///
/// Un service UPnP contient :
/// - Un identifiant unique (`identifier`)
/// - Une version (ex: 1, 2, 3...)
/// - Un ensemble d'actions ([`ActionSet`])
/// - Une table de variables d'état ([`StateVariableSet`])
///
/// # Cycle de vie
///
/// 1. Création avec [`Service::new`]
/// 2. Configuration (ajout d'actions et variables)
/// 3. Instanciation avec [`create_instance`](crate::UpnpModel::create_instance)
///
/// # Examples
///
/// ```rust
/// # use pmoupnp::services::Service;
/// # use pmoupnp::state_variables::StateVariable;
/// # use pmoupnp::variable_types::StateVarType;
/// # use std::sync::Arc;
/// let mut service = Service::new("ContentDirectory".to_string());
/// service.set_identifier("urn:upnp-org:serviceId:ContentDirectory".to_string());
/// service.set_version(1).unwrap();
///
/// // Ajouter une variable d'état
/// let search_caps = Arc::new(
///     StateVariable::new(StateVarType::String, "SearchCapabilities".to_string())
/// );
/// service.add_variable(search_caps);
/// ```
#[derive(Debug, Clone)]
pub struct Service {
    /// Métadonnées de l'objet UPnP
    object: UpnpObjectType,

    /// Identifiant du service (ex: "urn:upnp-org:serviceId:AVTransport")
    identifier: String,

    /// Version du service (>= 1)
    version: u32,

    /// Actions disponibles dans ce service
    actions: ActionSet,

    /// Variables d'état du service
    state_table: StateVariableSet,
}

impl Service {
    /// Crée un nouveau service UPnP.
    ///
    /// # Arguments
    ///
    /// * `name` - Nom du service (ex: "AVTransport", "RenderingControl")
    ///
    /// # Returns
    ///
    /// Un nouveau service avec :
    /// - Identifiant initialisé au nom
    /// - Version 1 par défaut
    /// - Collections vides d'actions et de variables
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// let service = Service::new("AVTransport".to_string());
    /// assert_eq!(service.name(), "AVTransport");
    /// assert_eq!(service.version(), 1);
    /// ```
    pub fn new(name: String) -> Self {
        Self {
            object: UpnpObjectType {
                name: name.clone(),
                object_type: "Service".to_string(),
            },
            identifier: name,
            version: 1,
            state_table: StateVariableSet::new(),
            actions: ActionSet::new(),
        }
    }

    /// Retourne le nom du service.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// let service = Service::new("AVTransport".to_string());
    /// assert_eq!(service.name(), "AVTransport");
    /// ```
    pub fn name(&self) -> &str {
        &self.object.name
    }

    /// Retourne le type d'objet.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// let service = Service::new("AVTransport".to_string());
    /// assert_eq!(service.type_id(), "Service");
    /// ```
    pub fn type_id(&self) -> &str {
        &self.object.object_type
    }

    /// Retourne l'identifiant du service.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// let mut service = Service::new("AVTransport".to_string());
    /// service.set_identifier("urn:upnp-org:serviceId:AVTransport".to_string());
    /// assert_eq!(service.identifier(), "urn:upnp-org:serviceId:AVTransport");
    /// ```
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Définit l'identifiant du service.
    ///
    /// # Arguments
    ///
    /// * `id` - Nouvel identifiant (typiquement un URN UPnP)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// let mut service = Service::new("AVTransport".to_string());
    /// service.set_identifier("urn:upnp-org:serviceId:AVTransport".to_string());
    /// ```
    pub fn set_identifier(&mut self, id: String) {
        self.identifier = id;
    }

    /// Retourne la version du service.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// let service = Service::new("AVTransport".to_string());
    /// assert_eq!(service.version(), 1);
    /// ```
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Définit la version du service.
    ///
    /// # Arguments
    ///
    /// * `version` - Numéro de version (doit être >= 1)
    ///
    /// # Errors
    ///
    /// Retourne une erreur si la version est < 1.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// let mut service = Service::new("AVTransport".to_string());
    /// assert!(service.set_version(2).is_ok());
    /// assert!(service.set_version(0).is_err());
    /// ```
    pub fn set_version(&mut self, version: u32) -> Result<(), ServiceError> {
        if version < 1 {
            return Err(ServiceError::ValidationError(
                "Version must be >= 1".to_string(),
            ));
        }
        self.version = version;
        Ok(())
    }

    /// Ajoute une variable d'état au service.
    ///
    /// # Arguments
    ///
    /// * `sv` - Variable d'état à ajouter
    ///
    /// # Errors
    ///
    /// Retourne une erreur si une variable avec le même nom existe déjà.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::state_variables::StateVariable;
    /// # use pmoupnp::variable_types::StateVarType;
    /// # use std::sync::Arc;
    /// let mut service = Service::new("AVTransport".to_string());
    /// let var = Arc::new(
    ///     StateVariable::new(StateVarType::String, "TransportState".to_string())
    /// );
    /// service.add_variable(var).unwrap();
    /// ```
    pub fn add_variable(
        &mut self,
        sv: Arc<crate::state_variables::StateVariable>,
    ) -> Result<(), ServiceError> {
        self.state_table
            .insert(sv)
            .map_err(|e| ServiceError::SetError(format!("Failed to add variable: {:?}", e)))
    }

    /// Vérifie si une variable d'état existe dans le service.
    ///
    /// # Arguments
    ///
    /// * `sv` - Variable à rechercher
    ///
    /// # Returns
    ///
    /// `true` si la variable existe, `false` sinon.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::state_variables::StateVariable;
    /// # use pmoupnp::variable_types::StateVarType;
    /// # use std::sync::Arc;
    /// let mut service = Service::new("AVTransport".to_string());
    /// let var = Arc::new(
    ///     StateVariable::new(StateVarType::String, "TransportState".to_string())
    /// );
    /// service.add_variable(var.clone()).unwrap();
    /// assert!(service.contains_variable(var));
    /// ```
    pub fn contains_variable(&self, sv: Arc<crate::state_variables::StateVariable>) -> bool {
        self.state_table.contains(sv)
    }

    /// Retourne toutes les variables d'état du service.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// let service = Service::new("AVTransport".to_string());
    /// for var in service.variables() {
    ///     println!("Variable: {}", var.get_name());
    /// }
    /// ```
    pub fn variables(&self) -> Vec<Arc<crate::state_variables::StateVariable>> {
        self.state_table.all()
    }

    /// Ajoute une action au service.
    ///
    /// # Arguments
    ///
    /// * `action` - Action à ajouter
    ///
    /// # Errors
    ///
    /// Retourne une erreur si une action avec le même nom existe déjà.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// # use pmoupnp::actions::Action;
    /// # use std::sync::Arc;
    /// let mut service = Service::new("AVTransport".to_string());
    /// let action = Arc::new(Action::new("Play".to_string()));
    /// service.add_action(action).unwrap();
    /// ```
    pub fn add_action(&mut self, action: Arc<crate::actions::Action>) -> Result<(), ServiceError> {
        self.actions
            .insert(action)
            .map_err(|e| ServiceError::SetError(format!("Failed to add action: {:?}", e)))
    }

    /// Retourne toutes les actions du service.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// let service = Service::new("AVTransport".to_string());
    /// for action in service.actions() {
    ///     println!("Action: {}", action.get_name());
    /// }
    /// ```
    pub fn actions(&self) -> Vec<Arc<crate::actions::Action>> {
        self.actions.all()
    }

    /// Retourne le type de service UPnP.
    ///
    /// Format: `urn:schemas-upnp-org:service:{name}:{version}`
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// let service = Service::new("AVTransport".to_string());
    /// assert_eq!(
    ///     service.service_type(),
    ///     "urn:schemas-upnp-org:service:AVTransport:1"
    /// );
    /// ```
    pub fn service_type(&self) -> String {
        format!(
            "urn:schemas-upnp-org:service:{}:{}",
            self.name(),
            self.version
        )
    }

    /// Retourne l'ideintifiant du service UPnP.
    ///
    /// Format: `urn:schemas-upnp-org:serviceId:{name}`
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use pmoupnp::services::Service;
    /// let service = Service::new("AVTransport".to_string());
    /// assert_eq!(
    ///     service.service_id(),
    ///     "urn:schemas-upnp-org:serviceId:AVTransport"
    /// );
    /// ```
    pub fn service_id(&self) -> String {
        format!("urn:schemas-upnp-org:serviceId:{}", self.name())
    }

    fn service_base_url(&self) -> String {
        format!("/service/{}", self.name())
    }

    pub fn scpd_route(&self) -> String {
        format!("{}/desc.xml", self.service_base_url())
    }

    pub fn control_route(&self) -> String {
        format!("{}/control", self.service_base_url())
    }

    pub fn event_route(&self) -> String {
        format!("{}/event", self.service_base_url())
    }

    pub fn scpd_element(&self) -> Element {
        let mut scpd = Element::new("scpd");
        scpd.attributes.insert(
            "xmlns".to_string(),
            "urn:schemas-upnp-org:service-1-0".to_string(),
        );

        let mut specversion = Element::new("specVersion");
        let mut major= Element::new("major");
        major.children
            .push(XMLNode::Text("1".to_string()));
        specversion.children.push(XMLNode::Element(major));
        let mut minor = Element::new("minor");
        minor.children
            .push(XMLNode::Text("0".to_string()));
        specversion.children.push(XMLNode::Element(minor));
        scpd.children.push(XMLNode::Element(specversion));

        scpd.children.push(XMLNode::Element(self.actions.to_xml_element()));
        scpd.children.push(XMLNode::Element(self.state_table.to_xml_element()));

        scpd
    }

    pub fn scpd_xml(&self) -> String {
        let elem = self.scpd_element();

        let config = EmitterConfig::new()
            .perform_indent(true)
            .indent_string("  ");

        let mut buf = Vec::new();
        elem.write_with_config(&mut buf, config)
            .expect("Failed to write XML");

        String::from_utf8(buf).expect("Invalid UTF-8")

    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::state_variables::StateVariable;
    use crate::variable_types::StateVarType;

    #[test]
    fn test_service_new() {
        let service = Service::new("AVTransport".to_string());
        assert_eq!(service.name(), "AVTransport");
        assert_eq!(service.type_id(), "Service");
        assert_eq!(service.version(), 1);
        assert_eq!(service.identifier(), "AVTransport");
    }

    #[test]
    fn test_service_set_version() {
        let mut service = Service::new("AVTransport".to_string());
        assert!(service.set_version(2).is_ok());
        assert_eq!(service.version(), 2);

        // Version 0 devrait échouer
        assert!(service.set_version(0).is_err());
    }

    #[test]
    fn test_service_add_variable() {
        let mut service = Service::new("AVTransport".to_string());
        let var = Arc::new(StateVariable::new(
            StateVarType::String,
            "TransportState".to_string(),
        ));

        assert!(service.add_variable(var.clone()).is_ok());
        assert!(service.contains_variable(var));
    }

    #[test]
    fn test_service_add_action() {
        let mut service = Service::new("AVTransport".to_string());
        let action = Arc::new(Action::new("Play".to_string()));

        assert!(service.add_action(action).is_ok());
        assert_eq!(service.actions().len(), 1);
    }

    #[test]
    fn test_service_type() {
        let mut service = Service::new("AVTransport".to_string());
        service.set_version(2).unwrap();

        assert_eq!(
            service.service_type(),
            "urn:schemas-upnp-org:service:AVTransport:2"
        );
    }
}
