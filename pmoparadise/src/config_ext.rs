//! Extension pour intégrer Radio Paradise dans pmoconfig
//!
//! Ce module fournit le trait `RadioParadiseConfigExt` qui permet d'ajouter facilement
//! des méthodes de gestion de la configuration Radio Paradise à pmoconfig::Config.
//!
//! # Exemple
//!
//! ```rust,ignore
//! use pmoconfig::get_config;
//! use pmoparadise::RadioParadiseConfigExt;
//!
//! let config = get_config();
//! let history_db = config.get_paradise_history_database()?;
//! let history_size = config.get_paradise_history_size()?;
//! ```

use anyhow::Result;
use pmoconfig::Config;
use serde_yaml::{Number, Value};

/// Chemin par défaut de la base de données d'historique (relatif au config_dir)
const DEFAULT_HISTORY_DATABASE: &str = "paradise_history.db";

/// Nombre maximal par défaut de pistes dans l'historique
const DEFAULT_HISTORY_SIZE: usize = 100;

/// Trait d'extension pour gérer la configuration Radio Paradise dans pmoconfig
///
/// Ce trait étend `pmoconfig::Config` avec des méthodes spécifiques
/// à la configuration de Radio Paradise (historique, etc.).
///
/// # Exemple
///
/// ```rust,ignore
/// use pmoconfig::get_config;
/// use pmoparadise::RadioParadiseConfigExt;
///
/// let config = get_config();
///
/// // Récupérer le chemin de la base de données d'historique
/// let db_path = config.get_paradise_history_database()?;
/// println!("History database: {}", db_path);
///
/// // Récupérer la taille maximale de l'historique
/// let max_tracks = config.get_paradise_history_size()?;
/// println!("Max history tracks: {}", max_tracks);
/// ```
pub trait RadioParadiseConfigExt {
    /// Récupère le chemin de la base de données d'historique
    ///
    /// Le chemin retourné est absolu, mais peut être configuré de manière relative
    /// au répertoire de configuration (via `get_managed_dir`).
    ///
    /// # Returns
    ///
    /// Le chemin absolu vers la base de données SQLite d'historique
    /// (default: `<config_dir>/paradise_history.db`)
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// let db_path = config.get_paradise_history_database()?;
    /// // Exemple: "/home/user/.config/pmo/paradise_history.db"
    /// ```
    fn get_paradise_history_database(&self) -> Result<String>;

    /// Définit le chemin de la base de données d'historique
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin de la base de données (absolu ou relatif au config_dir)
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// // Chemin relatif au config_dir
    /// config.set_paradise_history_database("my_paradise.db".to_string())?;
    ///
    /// // Ou chemin absolu
    /// config.set_paradise_history_database("/var/lib/paradise.db".to_string())?;
    /// ```
    fn set_paradise_history_database(&self, path: String) -> Result<()>;

    /// Récupère le nombre maximal de pistes dans l'historique
    ///
    /// # Returns
    ///
    /// Le nombre maximal de pistes à conserver dans l'historique (default: 100)
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
    /// // Conserver les 200 dernières pistes
    /// config.set_paradise_history_size(200)?;
    /// ```
    fn set_paradise_history_size(&self, size: usize) -> Result<()>;
}

impl RadioParadiseConfigExt for Config {
    fn get_paradise_history_database(&self) -> Result<String> {
        // Utilise get_managed_dir qui gère automatiquement les chemins
        // relatifs au config_dir et les chemins absolus
        self.get_managed_dir(
            &["sources", "radio_paradise", "history", "database"],
            DEFAULT_HISTORY_DATABASE,
        )
    }

    fn set_paradise_history_database(&self, path: String) -> Result<()> {
        self.set_managed_dir(&["sources", "radio_paradise", "history", "database"], path)
    }

    fn get_paradise_history_size(&self) -> Result<usize> {
        // Tente de lire depuis la configuration YAML
        match self.get_value(&["sources", "radio_paradise", "history", "max_tracks"]) {
            Ok(Value::Number(n)) if n.is_u64() => Ok(n.as_u64().unwrap() as usize),
            Ok(Value::Number(n)) if n.is_i64() => Ok(n.as_i64().unwrap() as usize),
            _ => Ok(DEFAULT_HISTORY_SIZE),
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
        // Les valeurs par défaut doivent être cohérentes
        assert_eq!(DEFAULT_HISTORY_DATABASE, "paradise_history.db");
        assert_eq!(DEFAULT_HISTORY_SIZE, 100);
    }
}
