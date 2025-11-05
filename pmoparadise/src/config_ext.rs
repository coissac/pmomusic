//! Extension pour intégrer Radio Paradise dans pmoconfig
//!
//! Ce module fournit le trait `RadioParadiseConfigExt` qui permet d'ajouter facilement
//! des méthodes de gestion de la configuration Radio Paradise à pmoconfig::Config.
//!
//! La configuration est minimale - seulement ce qui doit vraiment être configurable :
//! - Activation/désactivation de la source
//!
//! # Exemple
//!
//! ```rust,ignore
//! use pmoconfig::get_config;
//! use pmoparadise::RadioParadiseConfigExt;
//!
//! let config = get_config();
//!
//! // Check if enabled
//! if !config.get_paradise_enabled()? {
//!     println!("Radio Paradise is disabled");
//!     return Ok(());
//! }
//! ```

use anyhow::Result;
use pmoconfig::Config;
use serde_yaml::Value;

/// Trait d'extension pour gérer la configuration Radio Paradise dans pmoconfig
///
/// Ce trait étend `pmoconfig::Config` avec des méthodes spécifiques
/// à la configuration minimale de Radio Paradise.
///
/// # Auto-persist des valeurs par défaut
///
/// Le getter persiste automatiquement la valeur par défaut dans la
/// configuration si elle n'existe pas encore. Cela permet à l'utilisateur de
/// voir la configuration effective dans le fichier YAML et de la modifier facilement.
///
/// # Exemple
///
/// ```rust,ignore
/// use pmoconfig::get_config;
/// use pmoparadise::RadioParadiseConfigExt;
///
/// let config = get_config();
///
/// // Premier appel : persiste "enabled: true" dans la config et retourne true
/// let enabled = config.get_paradise_enabled()?;
///
/// // L'utilisateur peut maintenant éditer cette valeur dans le fichier YAML
/// ```
pub trait RadioParadiseConfigExt {
    /// Vérifie si Radio Paradise est activé
    ///
    /// # Returns
    ///
    /// `true` si la source est activée (default), `false` sinon.
    ///
    /// Si la valeur n'existe pas dans la configuration, elle est automatiquement
    /// définie à `true` (activé par défaut) et persistée.
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// if config.get_paradise_enabled()? {
    ///     // Initialize Radio Paradise...
    /// }
    /// ```
    fn get_paradise_enabled(&self) -> Result<bool>;

    /// Active ou désactive Radio Paradise
    ///
    /// # Arguments
    ///
    /// * `enabled` - `true` pour activer, `false` pour désactiver
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// // Disable Radio Paradise
    /// config.set_paradise_enabled(false)?;
    /// ```
    fn set_paradise_enabled(&self, enabled: bool) -> Result<()>;
}

impl RadioParadiseConfigExt for Config {
    fn get_paradise_enabled(&self) -> Result<bool> {
        match self.get_value(&["sources", "radio_paradise", "enabled"]) {
            Ok(Value::Bool(b)) => Ok(b),
            _ => {
                // Use default (enabled) and persist it
                self.set_paradise_enabled(true)?;
                Ok(true)
            }
        }
    }

    fn set_paradise_enabled(&self, enabled: bool) -> Result<()> {
        self.set_value(
            &["sources", "radio_paradise", "enabled"],
            Value::Bool(enabled),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_exists() {
        // Simple test to ensure the trait compiles
    }
}
