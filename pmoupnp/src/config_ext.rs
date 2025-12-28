//! Extension pour intégrer la configuration UPnP dans pmoconfig
//!
//! Ce module fournit le trait `UpnpConfigExt` qui permet d'ajouter facilement
//! des méthodes de configuration UPnP à pmoconfig::Config.
//!
//! Il suit le même pattern que pmocache/src/config_ext.rs pour la cohérence.

use anyhow::Result;
use pmoconfig::Config;
use serde_yaml::Value;

// Constantes par défaut pour les noms UPnP
const DEFAULT_MANUFACTURER: &str = "PMOMusic";
const DEFAULT_UDN_PREFIX: &str = "pmomusic";
const DEFAULT_MODEL_NAME_PREFIX: &str = "PMOMusic";
const DEFAULT_FRIENDLY_NAME_PREFIX: &str = "PMOMusic";

/// Trait d'extension pour ajouter la configuration UPnP à pmoconfig
///
/// Ce trait étend `pmoconfig::Config` avec des méthodes pour configurer
/// les noms et identifiants des devices UPnP.
///
/// # Exemple
///
/// ```rust,ignore
/// use pmoconfig::get_config;
/// use pmoupnp::UpnpConfigExt;
///
/// let config = get_config();
/// let manufacturer = config.get_upnp_manufacturer()?;
/// let udn_prefix = config.get_upnp_udn_prefix()?;
/// ```
pub trait UpnpConfigExt {
    /// Récupère le fabricant pour les devices UPnP
    ///
    /// # Returns
    ///
    /// Le nom du fabricant à afficher dans les descripteurs UPnP (défaut: "PMOMusic")
    fn get_upnp_manufacturer(&self) -> Result<String>;

    /// Définit le fabricant pour les devices UPnP
    fn set_upnp_manufacturer(&self, manufacturer: String) -> Result<()>;

    /// Récupère le préfixe UDN pour les devices UPnP
    ///
    /// # Returns
    ///
    /// Le préfixe utilisé pour générer les UDN (défaut: "pmomusic")
    fn get_upnp_udn_prefix(&self) -> Result<String>;

    /// Définit le préfixe UDN pour les devices UPnP
    fn set_upnp_udn_prefix(&self, prefix: String) -> Result<()>;

    /// Récupère le préfixe pour les noms de modèle des devices UPnP
    ///
    /// # Returns
    ///
    /// Le préfixe utilisé pour construire les model names (défaut: "PMOMusic")
    fn get_upnp_model_name_prefix(&self) -> Result<String>;

    /// Définit le préfixe pour les noms de modèle des devices UPnP
    fn set_upnp_model_name_prefix(&self, prefix: String) -> Result<()>;

    /// Récupère le préfixe pour les noms conviviaux des devices UPnP
    ///
    /// # Returns
    ///
    /// Le préfixe utilisé pour construire les friendly names (défaut: "PMOMusic")
    fn get_upnp_friendly_name_prefix(&self) -> Result<String>;

    /// Définit le préfixe pour les noms conviviaux des devices UPnP
    fn set_upnp_friendly_name_prefix(&self, prefix: String) -> Result<()>;
}

impl UpnpConfigExt for Config {
    fn get_upnp_manufacturer(&self) -> Result<String> {
        match self.get_value(&["host", "upnp", "manufacturer"]) {
            Ok(Value::String(s)) if !s.is_empty() => Ok(s),
            _ => Ok(DEFAULT_MANUFACTURER.to_string()),
        }
    }

    fn set_upnp_manufacturer(&self, manufacturer: String) -> Result<()> {
        self.set_value(
            &["host", "upnp", "manufacturer"],
            Value::String(manufacturer),
        )
    }

    fn get_upnp_udn_prefix(&self) -> Result<String> {
        match self.get_value(&["host", "upnp", "udn_prefix"]) {
            Ok(Value::String(s)) if !s.is_empty() => Ok(s),
            _ => Ok(DEFAULT_UDN_PREFIX.to_string()),
        }
    }

    fn set_upnp_udn_prefix(&self, prefix: String) -> Result<()> {
        self.set_value(&["host", "upnp", "udn_prefix"], Value::String(prefix))
    }

    fn get_upnp_model_name_prefix(&self) -> Result<String> {
        match self.get_value(&["host", "upnp", "model_name_prefix"]) {
            Ok(Value::String(s)) if !s.is_empty() => Ok(s),
            _ => Ok(DEFAULT_MODEL_NAME_PREFIX.to_string()),
        }
    }

    fn set_upnp_model_name_prefix(&self, prefix: String) -> Result<()> {
        self.set_value(
            &["host", "upnp", "model_name_prefix"],
            Value::String(prefix),
        )
    }

    fn get_upnp_friendly_name_prefix(&self) -> Result<String> {
        match self.get_value(&["host", "upnp", "friendly_name_prefix"]) {
            Ok(Value::String(s)) if !s.is_empty() => Ok(s),
            _ => Ok(DEFAULT_FRIENDLY_NAME_PREFIX.to_string()),
        }
    }

    fn set_upnp_friendly_name_prefix(&self, prefix: String) -> Result<()> {
        self.set_value(
            &["host", "upnp", "friendly_name_prefix"],
            Value::String(prefix),
        )
    }
}
