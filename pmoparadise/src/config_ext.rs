//! Extension pour intégrer Radio Paradise dans pmoconfig
//!
//! Ce module fournit le trait `RadioParadiseConfigExt` qui permet d'ajouter facilement
//! des méthodes de gestion de la configuration Radio Paradise à pmoconfig::Config.
//!
//! La configuration est minimale - seulement ce qui doit vraiment être configurable :
//! - Activation/désactivation de la source
//! - Chemin de la base de données d'historique
//! - Taille maximale de l'historique
//!
//! Tous les autres paramètres (polling, timeouts, etc.) sont des constantes
//! définies dans `paradise::constants`.
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
//!
//! // Get configuration
//! let db_path = config.get_paradise_history_database()?;
//! let max_tracks = config.get_paradise_history_size()?;
//! ```

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use pmoconfig::Config;
use serde_yaml::{Number, Value};

use crate::channels::HISTORY_DEFAULT_MAX_TRACKS;

/// Nom du répertoire pour Radio Paradise (relatif au config_dir)
///
/// La base de données sera stockée dans `<config_dir>/paradise/history.db`
const DEFAULT_HISTORY_DATABASE_DIR: &str = "paradise";

/// Trait d'extension pour gérer la configuration Radio Paradise dans pmoconfig
///
/// Ce trait étend `pmoconfig::Config` avec des méthodes spécifiques
/// à la configuration minimale de Radio Paradise.
///
/// # Auto-persist des valeurs par défaut
///
/// Tous les getters persistent automatiquement la valeur par défaut dans la
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
/// // Premier appel : persiste "max_tracks: 100" dans la config et retourne 100
/// let max_tracks = config.get_paradise_history_size()?;
///
/// // L'utilisateur peut maintenant éditer ces valeurs dans le fichier YAML
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

    /// Récupère le chemin de la base de données d'historique
    ///
    /// Le chemin retourné est absolu et pointe vers `<config_dir>/paradise/history.db`.
    /// Le répertoire `paradise` est créé automatiquement s'il n'existe pas.
    ///
    /// # Returns
    ///
    /// Le chemin absolu vers la base de données SQLite d'historique.
    /// Exemple: `/home/user/.config/pmo/paradise/history.db`
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// let db_path = config.get_paradise_history_database()?;
    /// let backend = SqliteHistoryBackend::new(&db_path)?;
    /// ```
    fn get_paradise_history_database(&self) -> Result<String>;

    /// Définit le chemin de la base de données d'historique
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin complet vers la base de données (doit inclure le nom du fichier)
    ///
    /// Le répertoire parent sera extrait et stocké dans la configuration.
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// // Set custom path
    /// config.set_paradise_history_database("/var/lib/pmo/paradise.db".to_string())?;
    /// ```
    fn set_paradise_history_database(&self, path: String) -> Result<()>;

    /// Récupère le nombre maximal de pistes dans l'historique
    ///
    /// # Returns
    ///
    /// Le nombre maximal de pistes à conserver dans l'historique.
    ///
    /// Si la valeur n'existe pas dans la configuration, elle est automatiquement
    /// définie à la constante `HISTORY_DEFAULT_MAX_TRACKS` (100) et persistée.
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// let max_tracks = config.get_paradise_history_size()?;
    /// println!("Keeping last {} tracks", max_tracks);
    /// ```
    fn get_paradise_history_size(&self) -> Result<usize>;

    /// Définit le nombre maximal de pistes dans l'historique
    ///
    /// # Arguments
    ///
    /// * `size` - Nombre maximal de pistes à conserver
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// // Keep last 200 tracks
    /// config.set_paradise_history_size(200)?;
    /// ```
    fn set_paradise_history_size(&self, size: usize) -> Result<()>;
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

    fn get_paradise_history_database(&self) -> Result<String> {
        // Get managed directory: ~/.config/pmo/paradise/
        let dir = self.get_managed_dir(
            &["sources", "radio_paradise", "database"],
            DEFAULT_HISTORY_DATABASE_DIR,
        )?;

        // Ensure directory exists
        std::fs::create_dir_all(&dir)?;

        // Build full path: ~/.config/pmo/paradise/history.db
        let mut path = PathBuf::from(dir);
        path.push("history.db");

        Ok(path.to_string_lossy().to_string())
    }

    fn set_paradise_history_database(&self, path: String) -> Result<()> {
        // Extract parent directory from the full path
        match PathBuf::from(&path).parent() {
            Some(dir) => self.set_managed_dir(
                &["sources", "radio_paradise", "database"],
                dir.to_string_lossy().to_string(),
            ),
            None => Err(anyhow!("Invalid database path: no parent directory")),
        }
    }

    fn get_paradise_history_size(&self) -> Result<usize> {
        match self.get_value(&["sources", "radio_paradise", "history", "max_tracks"]) {
            Ok(Value::Number(n)) if n.is_u64() => Ok(n.as_u64().unwrap() as usize),
            Ok(Value::Number(n)) if n.is_i64() => Ok(n.as_i64().unwrap() as usize),
            _ => {
                // Use default and persist it
                let default = HISTORY_DEFAULT_MAX_TRACKS;
                self.set_paradise_history_size(default)?;
                Ok(default)
            }
        }
    }

    fn set_paradise_history_size(&self, size: usize) -> Result<()> {
        let n = Number::from(size);
        self.set_value(
            &["sources", "radio_paradise", "history", "max_tracks"],
            Value::Number(n),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        assert_eq!(DEFAULT_HISTORY_DATABASE_DIR, "paradise");
        assert_eq!(HISTORY_DEFAULT_MAX_TRACKS, 100);
    }

    #[test]
    fn test_database_path_construction() {
        // Simulating path construction
        let base = "/home/user/.config/pmo/paradise";
        let mut path = PathBuf::from(base);
        path.push("history.db");

        assert_eq!(
            path.to_string_lossy(),
            "/home/user/.config/pmo/paradise/history.db"
        );
    }
}
