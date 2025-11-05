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

use crate::{channels::ParadiseChannelKind, client::DEFAULT_CHANNEL};
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
    /// Peut être configuré comme chaîne de caractères ou nombre :
    /// - "main" ou 0 = Main Mix (eclectic, diverse mix)
    /// - "mellow" ou 1 = Mellow Mix (smooth, chilled music)
    /// - "rock" ou 2 = Rock Mix (classic & modern rock)
    /// - "eclectic" ou 3 = Eclectic Mix (global sounds)
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
    fn get_paradise_default_channel(&self) -> Result<u8>;

    /// Définit le channel par défaut
    ///
    /// # Arguments
    ///
    /// * `channel` - Le channel (0-3)
    ///
    /// La valeur est stockée sous forme de nom convivial ("main", "mellow", etc.)
    /// dans le fichier de configuration.
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// use pmoparadise::channels::ParadiseChannelKind;
    ///
    /// // Use Mellow Mix by default
    /// config.set_paradise_default_channel(ParadiseChannelKind::Mellow.id())?;
    /// // Or simply:
    /// config.set_paradise_default_channel(1)?;
    /// ```
    fn set_paradise_default_channel(&self, channel: u8) -> Result<()>;
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

    fn get_paradise_default_channel(&self) -> Result<u8> {
        match self.get_value(&["sources", "radio_paradise", "default_channel"]) {
            Ok(Value::String(s)) => {
                // Try to parse as channel name (e.g., "main", "mellow", etc.)
                match s.parse::<ParadiseChannelKind>() {
                    Ok(kind) => Ok(kind.id()),
                    Err(_) => {
                        // Invalid channel name, use default
                        self.set_paradise_default_channel(DEFAULT_CHANNEL)?;
                        Ok(DEFAULT_CHANNEL)
                    }
                }
            }
            Ok(Value::Number(n)) => {
                // Accept numeric channel ID (0-3)
                if let Some(ch) = n.as_u64() {
                    if ch <= 3 {
                        Ok(ch as u8)
                    } else {
                        // Invalid channel number, use default
                        self.set_paradise_default_channel(DEFAULT_CHANNEL)?;
                        Ok(DEFAULT_CHANNEL)
                    }
                } else {
                    // Not a valid number, use default
                    self.set_paradise_default_channel(DEFAULT_CHANNEL)?;
                    Ok(DEFAULT_CHANNEL)
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

    fn set_paradise_default_channel(&self, channel: u8) -> Result<()> {
        // Convert channel ID to user-friendly string name
        let channel_name = match channel {
            0 => "main",
            1 => "mellow",
            2 => "rock",
            3 => "eclectic",
            _ => return Err(anyhow::anyhow!("Invalid channel ID: {}", channel)),
        };

        self.set_value(
            &["sources", "radio_paradise", "default_channel"],
            Value::String(channel_name.to_string()),
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
