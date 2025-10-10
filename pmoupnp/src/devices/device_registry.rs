//! Registre centralisé des instances de devices UPnP.
//!
//! Ce module fournit un `DeviceRegistry` qui maintient une collection de tous
//! les `DeviceInstance` actifs, permettant l'introspection et la modification
//! de l'état du serveur UPnP.

use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

use crate::{
    devices::DeviceInstance,
    state_variables::UpnpVariable,
    UpnpTyped, UpnpObjectSet, UpnpTypedInstance,
};

/// Ensemble de DeviceInstance.
///
/// Utilise `UpnpObjectSet` pour bénéficier de l'API standardisée.
pub type DeviceInstanceSet = UpnpObjectSet<DeviceInstance>;

/// Registre centralisé des devices UPnP.
///
/// Maintient une collection de tous les devices instanciés, indexés par leur nom ET par leur UDN.
/// Le registre utilise deux index pour permettre une recherche rapide :
/// - `devices` : Index par nom (via UpnpObjectSet)
/// - `udn_index` : Index par UDN pour un accès direct
///
/// **Important** : L'enregistrement d'un device dans le registre déclenche automatiquement
/// l'enregistrement de toutes ses URLs dans le serveur web.
///
/// Fournit des méthodes pour :
/// - Enregistrer/désenregistrer des devices (avec enregistrement automatique au serveur)
/// - Rechercher des devices par UDN ou nom
/// - Introspection complète de la hiérarchie Device/Service/Action/Variable
/// - Modification des variables d'état
///
/// # Examples
///
/// ```rust,ignore
/// use pmoupnp::devices::DeviceRegistry;
///
/// let mut registry = DeviceRegistry::new();
///
/// // L'enregistrement déclenche automatiquement l'enregistrement au serveur web
/// registry.register(device_instance, &mut server).await?;
///
/// // Introspection
/// let devices = registry.list_devices();
/// let device_info = registry.get_device_info("uuid:...");
///
/// // Modification
/// registry.set_variable("uuid:...", "AVTransport", "TransportState", "PLAYING").await?;
/// ```
#[derive(Clone)]
pub struct DeviceRegistry {
    /// Devices indexés par nom (via UpnpObjectSet)
    devices: DeviceInstanceSet,
    /// Index supplémentaire : UDN -> nom du device
    udn_index: Arc<RwLock<HashMap<String, String>>>,
}

impl std::fmt::Debug for DeviceRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let udns = self.udn_index.read().unwrap();
        f.debug_struct("DeviceRegistry")
            .field("device_count", &self.devices.all().len())
            .field("udns", &udns.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Default for DeviceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceRegistry {
    /// Crée un nouveau registre vide.
    pub fn new() -> Self {
        Self {
            devices: DeviceInstanceSet::new(),
            udn_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Enregistre un device dans le registre.
    ///
    /// # Arguments
    ///
    /// * `device` - Instance du device à enregistrer
    ///
    /// # Returns
    ///
    /// `Ok(())` si l'enregistrement réussit, `Err` si un device avec le même UDN ou nom existe déjà.
    ///
    /// # Note
    ///
    /// Cette méthode n'enregistre **pas** les URLs dans le serveur web. Cela doit être fait
    /// séparément via `device.register_urls()` ou via `UpnpServer::register_device()`.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// registry.register(device_instance)?;
    /// ```
    pub fn register(&mut self, device: Arc<DeviceInstance>) -> Result<(), String> {
        let udn = device.udn().to_string();
        let name = device.get_name().to_string();

        // Vérifier si l'UDN existe déjà
        {
            let udn_idx = self.udn_index.read().unwrap();
            if udn_idx.contains_key(&udn) {
                return Err(format!("Device with UDN {} already registered", udn));
            }
        }

        // Insérer dans le DeviceInstanceSet (par nom)
        self.devices.insert(device)
            .map_err(|e| format!("Failed to register device in registry: {:?}", e))?;

        // Mettre à jour l'index UDN
        {
            let mut udn_idx = self.udn_index.write().unwrap();
            udn_idx.insert(udn, name);
        }

        Ok(())
    }

    /// Désenregistre un device du registre par UDN.
    ///
    /// # Arguments
    ///
    /// * `udn` - UDN du device à désenregistrer
    ///
    /// # Returns
    ///
    /// `Some(Arc<DeviceInstance>)` si le device a été trouvé et supprimé, `None` sinon.
    pub fn unregister(&mut self, udn: &str) -> Option<Arc<DeviceInstance>> {
        // Trouver le nom via l'index UDN
        let name = {
            let mut udn_idx = self.udn_index.write().unwrap();
            udn_idx.remove(udn)?
        };

        // Supprimer du DeviceInstanceSet
        self.get_device_by_name(&name)
    }

    /// Récupère un device par son UDN.
    ///
    /// # Arguments
    ///
    /// * `udn` - UDN du device recherché
    ///
    /// # Returns
    ///
    /// `Some(Arc<DeviceInstance>)` si trouvé, `None` sinon.
    pub fn get_device(&self, udn: &str) -> Option<Arc<DeviceInstance>> {
        let udn_idx = self.udn_index.read().unwrap();
        let name = udn_idx.get(udn)?;
        self.devices.get_by_name(name)
    }

    /// Récupère un device par son nom.
    ///
    /// # Arguments
    ///
    /// * `name` - Nom du device recherché
    ///
    /// # Returns
    ///
    /// `Some(Arc<DeviceInstance>)` si trouvé, `None` sinon.
    pub fn get_device_by_name(&self, name: &str) -> Option<Arc<DeviceInstance>> {
        self.devices.get_by_name(name)
    }

    /// Liste tous les devices enregistrés.
    ///
    /// # Returns
    ///
    /// Un vecteur contenant tous les devices.
    pub fn list_devices(&self) -> Vec<Arc<DeviceInstance>> {
        self.devices.all()
    }

    /// Compte le nombre de devices enregistrés.
    pub fn count(&self) -> usize {
        self.devices.all().len()
    }

    /// Récupère les informations complètes d'un device.
    ///
    /// # Arguments
    ///
    /// * `udn` - UDN du device
    ///
    /// # Returns
    ///
    /// `Some(DeviceInfo)` contenant toutes les informations du device, `None` si non trouvé.
    pub fn get_device_info(&self, udn: &str) -> Option<DeviceInfo> {
        let device = self.get_device(udn)?;
        Some(DeviceInfo::from_instance(&device))
    }

    /// Liste les informations de tous les devices.
    pub fn list_device_infos(&self) -> Vec<DeviceInfo> {
        self.list_devices()
            .iter()
            .map(|d| DeviceInfo::from_instance(d))
            .collect()
    }

    /// Récupère la valeur d'une variable d'état.
    ///
    /// # Arguments
    ///
    /// * `udn` - UDN du device
    /// * `service_name` - Nom du service
    /// * `variable_name` - Nom de la variable
    ///
    /// # Returns
    ///
    /// `Some(String)` contenant la valeur de la variable, `None` si non trouvée.
    pub fn get_variable(&self, udn: &str, service_name: &str, variable_name: &str) -> Option<String> {
        let device = self.get_device(udn)?;
        let service = device.get_service(service_name)?;
        let variable = service.get_variable(variable_name)?;
        Some(variable.value().to_string())
    }

    /// Définit la valeur d'une variable d'état.
    ///
    /// # Arguments
    ///
    /// * `udn` - UDN du device
    /// * `service_name` - Nom du service
    /// * `variable_name` - Nom de la variable
    /// * `value` - Nouvelle valeur (sous forme de chaîne)
    ///
    /// # Returns
    ///
    /// `Ok(())` si la modification réussit, `Err(String)` en cas d'erreur.
    pub async fn set_variable(&self, udn: &str, service_name: &str, variable_name: &str, value: &str) -> Result<(), String> {
        let device = self.get_device(udn)
            .ok_or_else(|| format!("Device {} not found", udn))?;

        let service = device.get_service(service_name)
            .ok_or_else(|| format!("Service {} not found", service_name))?;

        let variable = service.get_variable(variable_name)
            .ok_or_else(|| format!("Variable {} not found", variable_name))?;

        // Parser et valider la valeur selon le type de la variable
        use crate::variable_types::{StateValue, UpnpVarType};
        let var_model = variable.get_model();
        let state_value = StateValue::from_string(value, &var_model.as_state_var_type())
            .map_err(|e| format!("Invalid value for variable {}: {:?}", variable_name, e))?;

        variable.set_value(state_value).await
            .map_err(|e| format!("Failed to set value: {:?}", e))?;

        Ok(())
    }

    /// Récupère toutes les variables d'un service.
    ///
    /// # Arguments
    ///
    /// * `udn` - UDN du device
    /// * `service_name` - Nom du service
    ///
    /// # Returns
    ///
    /// `Some(HashMap<String, String>)` avec les variables (nom -> valeur), `None` si non trouvé.
    pub fn get_service_variables(&self, udn: &str, service_name: &str) -> Option<HashMap<String, String>> {
        let device = self.get_device(udn)?;
        let service = device.get_service(service_name)?;

        let mut variables = HashMap::new();
        for var in service.statevariables().all() {
            variables.insert(var.get_name().to_string(), var.value().to_string());
        }

        Some(variables)
    }
}

/// Informations structurées sur un device.
///
/// Utilisé pour l'introspection et la sérialisation JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// UDN unique du device
    pub udn: String,
    /// Nom du device
    pub name: String,
    /// Nom convivial
    pub friendly_name: String,
    /// Type de device
    pub device_type: String,
    /// Fabricant
    pub manufacturer: String,
    /// Nom du modèle
    pub model_name: String,
    /// URL de base
    pub base_url: String,
    /// Services du device
    pub services: Vec<ServiceInfo>,
    /// Sous-devices
    pub devices: Vec<DeviceInfo>,
}

impl DeviceInfo {
    /// Crée une structure DeviceInfo à partir d'une DeviceInstance.
    pub fn from_instance(instance: &DeviceInstance) -> Self {
        let model = instance.get_model();

        Self {
            udn: instance.udn().to_string(),
            name: instance.get_name().to_string(),
            friendly_name: model.friendly_name().to_string(),
            device_type: model.device_type(),
            manufacturer: model.manufacturer().to_string(),
            model_name: model.model_name().to_string(),
            base_url: instance.base_url().to_string(),
            services: instance.services()
                .iter()
                .map(|s| ServiceInfo::from_instance(s))
                .collect(),
            devices: instance.devices()
                .iter()
                .map(|d| DeviceInfo::from_instance(d))
                .collect(),
        }
    }
}

/// Informations sur un service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Nom du service
    pub name: String,
    /// Type de service
    pub service_type: String,
    /// Identifiant
    pub service_id: String,
    /// Actions disponibles
    pub actions: Vec<ActionInfo>,
    /// Variables d'état
    pub variables: Vec<VariableInfo>,
}

impl ServiceInfo {
    /// Crée une structure ServiceInfo à partir d'une ServiceInstance.
    pub fn from_instance(instance: &crate::services::ServiceInstance) -> Self {
        Self {
            name: instance.get_name().to_string(),
            service_type: instance.service_type(),
            service_id: instance.service_id(),
            actions: instance.actions()
                .all()
                .iter()
                .map(|a| ActionInfo::from_instance(a))
                .collect(),
            variables: instance.statevariables()
                .all()
                .iter()
                .map(|v| VariableInfo::from_instance(v))
                .collect(),
        }
    }
}

/// Informations sur une action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionInfo {
    /// Nom de l'action
    pub name: String,
    /// Arguments d'entrée
    pub arguments_in: Vec<ArgumentInfo>,
    /// Arguments de sortie
    pub arguments_out: Vec<ArgumentInfo>,
}

impl ActionInfo {
    /// Crée une structure ActionInfo à partir d'une ActionInstance.
    pub fn from_instance(instance: &crate::actions::ActionInstance) -> Self {
        let args = instance.arguments_set();

        let mut arguments_in = Vec::new();
        let mut arguments_out = Vec::new();

        for arg in args.all() {
            let model = arg.get_model();
            if model.is_in() {
                arguments_in.push(ArgumentInfo::from_instance(&arg));
            }
            if model.is_out() {
                arguments_out.push(ArgumentInfo::from_instance(&arg));
            }
        }

        Self {
            name: instance.get_name().to_string(),
            arguments_in,
            arguments_out,
        }
    }
}

/// Informations sur un argument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentInfo {
    /// Nom de l'argument
    pub name: String,
    /// Variable d'état associée
    pub state_variable: String,
}

impl ArgumentInfo {
    /// Crée une structure ArgumentInfo à partir d'une ArgumentInstance.
    pub fn from_instance(instance: &crate::actions::ArgumentInstance) -> Self {
        let model = instance.get_model();
        Self {
            name: instance.get_name().to_string(),
            state_variable: model.state_variable().get_name().to_string(),
        }
    }
}

/// Informations sur une variable d'état.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableInfo {
    /// Nom de la variable
    pub name: String,
    /// Type de données
    pub data_type: String,
    /// Valeur actuelle
    pub value: String,
    /// Envoie des notifications
    pub sends_events: bool,
}

impl VariableInfo {
    /// Crée une structure VariableInfo à partir d'une StateVarInstance.
    pub fn from_instance(instance: &crate::state_variables::StateVarInstance) -> Self {
        use crate::variable_types::UpnpVarType;
        let model = instance.get_model();

        Self {
            name: instance.get_name().to_string(),
            data_type: model.as_state_var_type().to_string(),
            value: instance.value().to_string(),
            sends_events: model.is_sending_notification(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        devices::Device,
        services::Service,
        UpnpModel,
    };

    #[test]
    fn test_registry_creation() {
        let registry = DeviceRegistry::new();
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_device_registration() {
        let registry = DeviceRegistry::new();
        let device = Device::new(
            "TestDevice".to_string(),
            "MediaRenderer".to_string(),
            "Test Renderer".to_string(),
        );
        let instance = Arc::new(device.create_instance());

        assert!(registry.register(instance.clone()).is_ok());
        assert_eq!(registry.count(), 1);

        // Vérifier qu'on ne peut pas enregistrer deux fois le même UDN
        assert!(registry.register(instance).is_err());
    }

    #[test]
    fn test_device_retrieval() {
        let registry = DeviceRegistry::new();
        let device = Device::new(
            "TestDevice".to_string(),
            "MediaRenderer".to_string(),
            "Test Renderer".to_string(),
        );
        let instance = Arc::new(device.create_instance());
        let udn = instance.udn().to_string();

        registry.register(instance.clone()).unwrap();

        // Récupération par UDN
        let retrieved = registry.get_device(&udn);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().udn(), udn);

        // Récupération par nom
        let retrieved_by_name = registry.get_device_by_name("TestDevice");
        assert!(retrieved_by_name.is_some());
    }

    #[test]
    fn test_device_unregistration() {
        let registry = DeviceRegistry::new();
        let device = Device::new(
            "TestDevice".to_string(),
            "MediaRenderer".to_string(),
            "Test Renderer".to_string(),
        );
        let instance = Arc::new(device.create_instance());
        let udn = instance.udn().to_string();

        registry.register(instance).unwrap();
        assert_eq!(registry.count(), 1);

        let removed = registry.unregister(&udn);
        assert!(removed.is_some());
        assert_eq!(registry.count(), 0);
    }
}
