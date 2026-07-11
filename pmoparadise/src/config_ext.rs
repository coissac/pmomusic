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

use crate::{
    channels::{channel_by_id, resolve_channel},
    client::DEFAULT_CHANNEL,
};
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

    /// Récupère le channel par défaut
    ///
    /// # Returns
    ///
    /// Le channel par défaut (0 = Main Mix par défaut).
    ///
    /// Si la valeur n'existe pas dans la configuration, elle est automatiquement
    /// définie à "main" et persistée.
    ///
    /// # Channels disponibles
    ///
    /// Peut être configuré comme chaîne de caractères (slug) ou nombre (ID).
    /// La liste des canaux est dynamique (voir `channels::channels()`) :
    /// par exemple "main"/0, "mellow"/1, "rock"/2, "eclectic"/3, "beyond"/5,
    /// "serenity"/42, "kfat"/945.
    ///
    /// # Exemple de configuration YAML
    ///
    /// ```yaml
    /// sources:
    ///   radio_paradise:
    ///     default_channel: mellow  # or 1
    /// ```
    ///
    /// # Exemple d'utilisation
    ///
    /// ```rust,ignore
    /// let channel = config.get_paradise_default_channel()?;
    /// let client = RadioParadiseClient::builder().channel(channel).build().await?;
    /// ```
    fn get_paradise_default_channel(&self) -> Result<u16>;

    /// Définit le channel par défaut
    ///
    /// # Arguments
    ///
    /// * `channel` - L'ID du channel (doit exister dans le registre de canaux)
    ///
    /// La valeur est stockée sous forme de nom convivial ("main", "mellow", etc.)
    /// dans le fichier de configuration.
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// // Use Mellow Mix by default
    /// config.set_paradise_default_channel(1)?;
    /// ```
    fn set_paradise_default_channel(&self, channel: u16) -> Result<()>;
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

    fn get_paradise_default_channel(&self) -> Result<u16> {
        match self.get_value(&["sources", "radio_paradise", "default_channel"]) {
            Ok(Value::String(s)) => {
                // Slug ("main", "mellow", ...) ou ID numérique en chaîne
                match resolve_channel(&s) {
                    Some(descriptor) => Ok(descriptor.id),
                    None => {
                        // Invalid channel name, use default
                        self.set_paradise_default_channel(DEFAULT_CHANNEL)?;
                        Ok(DEFAULT_CHANNEL)
                    }
                }
            }
            Ok(Value::Number(n)) => {
                // Accept numeric channel ID (must exist in the registry)
                match n
                    .as_u64()
                    .and_then(|ch| u16::try_from(ch).ok())
                    .and_then(channel_by_id)
                {
                    Some(descriptor) => Ok(descriptor.id),
                    None => {
                        // Invalid channel number, use default
                        self.set_paradise_default_channel(DEFAULT_CHANNEL)?;
                        Ok(DEFAULT_CHANNEL)
                    }
                }
            }
            _ => {
                // Use default and persist it as "main" (user-friendly)
                self.set_value(
                    &["sources", "radio_paradise", "default_channel"],
                    Value::String("main".to_string()),
                )?;
                Ok(DEFAULT_CHANNEL)
            }
        }
    }

    fn set_paradise_default_channel(&self, channel: u16) -> Result<()> {
        // Convert channel ID to user-friendly slug
        let descriptor = channel_by_id(channel)
            .ok_or_else(|| anyhow::anyhow!("Invalid channel ID: {}", channel))?;

        self.set_value(
            &["sources", "radio_paradise", "default_channel"],
            Value::String(descriptor.slug),
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
