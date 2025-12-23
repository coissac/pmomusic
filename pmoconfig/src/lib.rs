//! # PMOMusic Configuration Module
//!
//! This module provides configuration management for PMOMusic, including:
//! - Loading configuration from YAML files
//! - Merging with embedded default configuration
//! - Environment variable overrides
//! - Type-safe getters and setters for configuration values
//! - Thread-safe singleton access pattern
//!
//! ## Usage
//!
//! ```no_run
//! use pmoconfig::get_config;
//!
//! // Get the global configuration
//! let config = get_config();
//!
//! // Access configuration values
//! let port = config.get_http_port();
//! let cache_dir = config.get_cover_cache_dir()?;
//!
//! // Update configuration values
//! config.set_http_port(9000)?;
//! # Ok::<(), anyhow::Error>(())
//! ```

use anyhow::{anyhow, Result};
use dirs::home_dir;
use lazy_static::lazy_static;
use pmoutils::guess_local_ip;
use serde_yaml::{Mapping, Number, Value};
use std::{
    env, fs,
    path::Path,
    sync::{Arc, Mutex},
};
use tracing::info;
use uuid::Uuid;

// Module de chiffrement des mots de passe
pub mod encryption;

// Modules conditionnels pour l'API REST
#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "api")]
pub mod openapi;

#[cfg(feature = "api")]
pub use openapi::ApiDoc;

// Configuration par défaut intégrée
const DEFAULT_CONFIG: &str = include_str!("pmomusic.yaml");

lazy_static! {
    static ref CONFIG: Arc<Config> =
        Arc::new(Config::load_config("").expect("Failed to load PMOMusic configuration"));
}

const ENV_CONFIG_DIR: &str = "PMOMUSIC_CONFIG";
const ENV_PREFIX: &str = "PMOMUSIC_CONFIG__";

// Default values for configuration
const DEFAULT_HTTP_PORT: u16 = 8080;
const DEFAULT_LOG_BUFFER_CAPACITY: usize = 1000;
const DEFAULT_LOG_MIN_LEVEL: &str = "TRACE";
const DEFAULT_LOG_ENABLE_CONSOLE: bool = true;

/// Macro to generate getter/setter for usize values with default
macro_rules! impl_usize_config {
    ($getter:ident, $setter:ident, $path:expr, $default:expr) => {
        pub fn $getter(&self) -> Result<usize> {
            match self.get_value($path)? {
                Value::Number(n) if n.is_i64() => Ok(n.as_i64().unwrap() as usize),
                Value::Number(n) if n.is_u64() => Ok(n.as_u64().unwrap() as usize),
                _ => Ok($default),
            }
        }

        pub fn $setter(&self, size: usize) -> Result<()> {
            let n = Number::from(size);
            self.set_value($path, Value::Number(n))
        }
    };
}

/// Macro to generate getter/setter for bool values with default
macro_rules! impl_bool_config {
    ($getter:ident, $setter:ident, $path:expr, $default:expr) => {
        pub fn $getter(&self) -> Result<bool> {
            match self.get_value($path)? {
                Value::Bool(b) => Ok(b),
                _ => Ok($default),
            }
        }

        pub fn $setter(&self, value: bool) -> Result<()> {
            self.set_value($path, Value::Bool(value))
        }
    };
}

/// Configuration manager for PMOMusic
///
/// This structure manages the application configuration, including:
/// - Loading configuration from YAML files
/// - Merging with default configuration
/// - Handling environment variable overrides
/// - Providing typed getters/setters for configuration values
///
/// # Examples
///
/// ```no_run
/// use pmoconfig::get_config;
///
/// let config = get_config();
/// let port = config.get_http_port();
/// println!("HTTP port: {}", port);
/// ```
#[derive(Debug)]
pub struct Config {
    config_dir: String,
    path: String,
    data: Mutex<Value>,
}

// Implémentation manuelle de Clone
impl Clone for Config {
    fn clone(&self) -> Self {
        let data = self.data.lock().unwrap().clone();
        Self {
            config_dir: self.config_dir.clone(),
            path: self.path.clone(),
            data: Mutex::new(data),
        }
    }
}

impl Config {
    /// Finds a config directory by trying different locations in order
    fn find_config_dir(directory: &str) -> String {
        // 1. Try provided directory
        if !directory.is_empty() {
            return directory.to_string();
        }

        // 2. Try environment variable
        if let Ok(env_path) = env::var(ENV_CONFIG_DIR) {
            info!(env_var=ENV_CONFIG_DIR, path=%env_path, "Trying to load config from env");
            return env_path;
        }

        // 3. Try current directory
        if Path::new(".pmomusic").exists() {
            return ".pmomusic".to_string();
        }

        // 4. Try home directory
        if let Some(home) = home_dir() {
            let home_config = home.join(".pmomusic");
            if home_config.exists() {
                return home_config.to_string_lossy().to_string();
            }
        }

        // Default fallback
        ".pmomusic".to_string()
    }

    /// Validates and prepares a config directory
    fn validate_config_dir(path: &Path) -> Result<()> {
        // Create if doesn't exist
        if !path.exists() {
            fs::create_dir_all(path)?;
        }

        // Verify it's a directory
        if !path.is_dir() {
            return Err(anyhow!("Le chemin spécifié n'est pas un répertoire"));
        }

        // Test write permission
        let test_file = path.join(".write_test");
        fs::write(&test_file, b"test")?;
        fs::remove_file(&test_file)?;

        // Test read permission
        fs::read_dir(path)?;

        Ok(())
    }

    /// Determines and validates the configuration directory
    ///
    /// The directory is searched in the following order:
    /// 1. The provided `directory` parameter if not empty
    /// 2. The `PMOMUSIC_CONFIG` environment variable
    /// 3. `.pmomusic` in the current directory
    /// 4. `.pmomusic` in the user's home directory
    ///
    /// The directory is created if it doesn't exist, and validated for read/write permissions.
    ///
    /// # Panics
    ///
    /// Panics if the directory cannot be created or validated
    pub fn config_dir(directory: &str) -> String {
        let dir_path = Self::find_config_dir(directory);
        let path = Path::new(&dir_path);

        Self::validate_config_dir(path)
            .expect("Impossible de valider le répertoire de configuration");

        dir_path
    }

    /// Loads the configuration from the specified directory
    ///
    /// This method:
    /// 1. Determines the configuration directory
    /// 2. Loads the default embedded configuration
    /// 3. Merges it with the external config.yaml file if present
    /// 4. Applies environment variable overrides
    /// 5. Saves the merged configuration
    ///
    /// # Arguments
    ///
    /// * `directory` - The directory containing the config.yaml file, or empty to use defaults
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the loaded `Config` or an error
    pub fn load_config(directory: &str) -> Result<Self> {
        // Obtenir le répertoire de configuration
        let config_dir = Self::config_dir(directory);
        info!(config_dir=%config_dir, "Using config directory");

        // Construire le chemin du fichier config.yaml
        let config_file_path = Path::new(&config_dir).join("config.yaml");
        let path = config_file_path.to_string_lossy().to_string();

        // Charger la configuration par défaut
        let mut default_value: Value = serde_yaml::from_str(DEFAULT_CONFIG)?;

        // Essayer de charger le fichier de configuration
        let yaml_data = if let Ok(data) = fs::read(&path) {
            info!(config_file=%path, "Loaded config file");
            data
        } else {
            info!(config_file=%path, "Config file not found, using default embedded config");
            DEFAULT_CONFIG.as_bytes().to_vec()
        };

        // Merger avec la config par défaut
        let external_value: Value = serde_yaml::from_slice(&yaml_data)?;
        merge_yaml(&mut default_value, &external_value);
        let mut config_value = Self::lower_keys_value(default_value);

        // Appliquer les overrides depuis les variables d'environnement
        Self::apply_env_overrides(&mut config_value);

        // Créer la configuration
        let config = Config {
            config_dir,
            path,
            data: Mutex::new(config_value),
        };

        // Sauvegarder la configuration
        config.save()?;
        Ok(config)
    }

    /// Saves the current configuration to the config.yaml file
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating success or failure
    pub fn save(&self) -> Result<()> {
        let data = self.data.lock().unwrap();
        let yaml = serde_yaml::to_string(&*data)?;
        fs::write(&self.path, yaml)?;
        Ok(())
    }

    /// Sets a configuration value at the specified path and saves it
    ///
    /// # Arguments
    ///
    /// * `path` - Array of keys representing the path (e.g., `&["host", "http_port"]`)
    /// * `value` - The YAML value to set
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating success or failure
    pub fn set_value(&self, path: &[&str], value: Value) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        Self::set_value_internal(&mut data, path, value.clone())?;
        drop(data);
        self.save()?;
        Ok(())
    }

    fn set_value_internal(data: &mut Value, path: &[&str], value: Value) -> Result<()> {
        if path.is_empty() {
            *data = value;
            return Ok(());
        }
        if let Value::Mapping(map) = data {
            let key = path[0].to_lowercase();
            let key_value = Value::String(key.clone());
            if path.len() == 1 {
                map.insert(key_value, value);
            } else {
                let entry = map
                    .entry(key_value)
                    .or_insert(Value::Mapping(Mapping::new()));
                Self::set_value_internal(entry, &path[1..], value)?;
            }
            Ok(())
        } else {
            Err(anyhow!("Current node is not a map"))
        }
    }

    /// Gets a configuration value at the specified path
    ///
    /// # Arguments
    ///
    /// * `path` - Array of keys representing the path (e.g., `&["host", "http_port"]`)
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the YAML value or an error if the path doesn't exist
    pub fn get_value(&self, path: &[&str]) -> Result<Value> {
        let data = self.data.lock().unwrap();
        Self::get_value_internal(&data, path)
    }

    fn get_value_internal(data: &Value, path: &[&str]) -> Result<Value> {
        let mut current = data;
        for (i, key) in path.iter().enumerate() {
            if let Value::Mapping(map) = current {
                let key = key.to_lowercase();

                if let Some(next) = map.get(&Value::String(key)) {
                    current = next;
                } else {
                    return Err(anyhow!("Path {} does not exist", path[..=i].join(".")));
                }
            } else {
                return Err(anyhow!("Path {} is not a Config", path[..i].join(".")));
            }
        }
        Ok(current.clone())
    }

    fn apply_env_overrides(config: &mut Value) {
        for (key, value) in env::vars() {
            if key.starts_with(ENV_PREFIX) {
                let key_path = key
                    .trim_start_matches(ENV_PREFIX)
                    .split("__")
                    .collect::<Vec<_>>();
                let yaml_value = Self::convert_env_value(&value);
                let _ = Self::set_value_internal(config, &key_path, yaml_value);
            }
        }
    }

    fn convert_env_value(value: &str) -> Value {
        if let Ok(parsed) = serde_yaml::from_str::<Value>(value) {
            return parsed;
        }
        Value::String(value.to_string())
    }

    fn lower_keys_value(value: Value) -> Value {
        match value {
            Value::Mapping(map) => {
                let mut new_map = Mapping::new();
                for (k, v) in map {
                    if let Value::String(s) = k {
                        let new_key = Value::String(s.to_lowercase());
                        let new_val = Self::lower_keys_value(v);
                        new_map.insert(new_key, new_val);
                    } else {
                        new_map.insert(k, Self::lower_keys_value(v));
                    }
                }
                Value::Mapping(new_map)
            }
            Value::Sequence(seq) => {
                Value::Sequence(seq.into_iter().map(Self::lower_keys_value).collect())
            }
            _ => value,
        }
    }

    /// Résout un chemin relatif ou absolu et crée le répertoire si nécessaire
    fn resolve_and_create_dir(&self, dir_path: &str) -> Result<String> {
        let path = Path::new(dir_path);

        // Déterminer si le chemin est relatif ou absolu
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            // Chemin relatif : le résoudre par rapport à config_dir
            Path::new(&self.config_dir).join(path)
        };

        // Créer le répertoire s'il n'existe pas
        if !absolute_path.exists() {
            fs::create_dir_all(&absolute_path)?;
            info!(directory=%absolute_path.display(), "Created cache directory");
        }

        // Retourner le chemin absolu
        Ok(absolute_path.to_string_lossy().to_string())
    }

    /// Récupère un répertoire géré par la configuration
    ///
    /// Cette méthode générique permet de récupérer n'importe quel répertoire
    /// configuré dans le YAML. Le répertoire peut être absolu ou relatif au
    /// répertoire de configuration. Il sera créé s'il n'existe pas.
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin dans l'arbre de configuration (ex: `&["host", "cache", "directory"]`)
    /// * `default` - Nom de répertoire par défaut si non configuré
    ///
    /// # Returns
    ///
    /// Le chemin absolu du répertoire, créé s'il n'existait pas
    ///
    /// # Exemple
    ///
    /// ```no_run
    /// use pmoconfig::get_config;
    ///
    /// let config = get_config();
    /// let cache_dir = config.get_managed_dir(&["host", "audio_cache", "directory"], "cache_audio")?;
    /// println!("Audio cache directory: {}", cache_dir);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn get_managed_dir(&self, path: &[&str], default: &str) -> Result<String> {
        let dir_path = match self.get_value(path) {
            Ok(Value::String(s)) => s,
            _ => {
                self.set_managed_dir(path, default.to_string())?;
                default.to_string()
            }
        };
        self.resolve_and_create_dir(&dir_path)
    }

    /// Définit un répertoire géré par la configuration
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin dans l'arbre de configuration (ex: `&["host", "cache", "directory"]`)
    /// * `directory` - Chemin du répertoire (absolu ou relatif au config_dir)
    ///
    /// # Exemple
    ///
    /// ```no_run
    /// use pmoconfig::get_config;
    ///
    /// let config = get_config();
    /// config.set_managed_dir(&["host", "audio_cache", "directory"], "/var/cache/audio".to_string())?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn set_managed_dir(&self, path: &[&str], directory: String) -> Result<()> {
        self.set_value(path, Value::String(directory))
    }

    /// Gets the base URL for the HTTP server
    ///
    /// Returns the configured base URL, or attempts to guess the local IP address if not configured.
    ///
    /// # Returns
    ///
    /// The base URL as a String
    pub fn get_base_url(&self) -> String {
        match self.get_value(&["host", "base_url"]) {
            Ok(Value::String(s)) if !s.is_empty() => s,
            Ok(_) => {
                tracing::warn!("Base URL is not a string or empty, using default localhost");
                guess_local_ip()
            }
            Err(err) => {
                tracing::warn!("Failed to get base URL: {}, using default localhost", err);
                guess_local_ip()
            }
        }
    }

    /// Gets the HTTP port from configuration
    ///
    /// Returns the configured HTTP port, or the default port (8080) if not configured or invalid.
    ///
    /// # Returns
    ///
    /// The HTTP port as a u16
    pub fn get_http_port(&self) -> u16 {
        match self.get_value(&["host", "http_port"]) {
            Ok(Value::Number(n)) if n.is_i64() => n.as_i64().unwrap() as u16,
            Ok(Value::String(s)) => match s.parse::<u16>() {
                Ok(port) => port,
                Err(_) => {
                    tracing::warn!(
                        "Invalid HTTP port '{}', using default {}",
                        s,
                        DEFAULT_HTTP_PORT
                    );
                    DEFAULT_HTTP_PORT
                }
            },
            Ok(_) => {
                tracing::warn!(
                    "HTTP port not a number or string, using default {}",
                    DEFAULT_HTTP_PORT
                );
                DEFAULT_HTTP_PORT
            }
            Err(err) => {
                tracing::warn!(
                    "Failed to get HTTP port: {}, using default {}",
                    err,
                    DEFAULT_HTTP_PORT
                );
                DEFAULT_HTTP_PORT
            }
        }
    }

    /// Sets the HTTP port in configuration
    ///
    /// # Arguments
    ///
    /// * `port` - The port number to set
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating success or failure
    pub fn set_http_port(&self, port: u16) -> Result<()> {
        let n = Number::from(port);
        self.set_value(&["host", "http_port"], Value::Number(n))
    }

    /// Gets the UDN (Unique Device Name) for a device, generating one if it doesn't exist
    ///
    /// # Arguments
    ///
    /// * `devtype` - The device type (e.g., "mediarenderer")
    /// * `name` - The device name
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the UDN string, generating a new UUID if not found
    pub fn get_device_udn(&self, devtype: &str, name: &str) -> Result<String> {
        let path = &["devices", devtype, name, "udn"];
        match self.get_value(path) {
            Ok(Value::String(udn)) => {
                let udn_str = udn.trim();
                let sanitized = udn_str.strip_prefix("uuid:").unwrap_or(udn_str).to_string();
                Ok(sanitized)
            }
            _ => {
                let new_udn = Uuid::new_v4().to_string();
                self.set_value(path, Value::String(new_udn.clone()))?;
                Ok(new_udn)
            }
        }
    }

    /// Sets the UDN (Unique Device Name) for a device
    ///
    /// # Arguments
    ///
    /// * `devtype` - The device type (e.g., "mediarenderer")
    /// * `name` - The device name
    /// * `udn` - The UDN to set
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating success or failure
    pub fn set_device_udn(&self, devtype: &str, name: &str, udn: String) -> Result<()> {
        let udn_str = udn.trim();
        let sanitized = udn_str.strip_prefix("uuid:").unwrap_or(udn_str).to_string();
        self.set_value(&["devices", devtype, name, "udn"], Value::String(sanitized))
    }

    impl_usize_config!(
        get_log_cache_size,
        set_log_cache_size,
        &["host", "logger", "buffer_capacity"],
        DEFAULT_LOG_BUFFER_CAPACITY
    );

    impl_bool_config!(
        get_log_enable_console,
        set_log_enable_console,
        &["host", "logger", "enable_console"],
        DEFAULT_LOG_ENABLE_CONSOLE
    );

    /// Récupère le niveau de log minimum depuis la configuration
    pub fn get_log_min_level(&self) -> Result<String> {
        match self.get_value(&["host", "logger", "min_level"])? {
            Value::String(s) => Ok(s),
            _ => Ok(DEFAULT_LOG_MIN_LEVEL.to_string()),
        }
    }

    /// Définit le niveau de log minimum dans la configuration
    pub fn set_log_min_level(&self, level: String) -> Result<()> {
        self.set_value(&["host", "logger", "min_level"], Value::String(level))
    }
}

/// Returns the global configuration instance
///
/// This function provides access to the singleton configuration instance,
/// which is lazily loaded on first access.
///
/// # Returns
///
/// An `Arc<Config>` pointing to the global configuration
///
/// # Examples
///
/// ```no_run
/// use pmoconfig::get_config;
///
/// let config = get_config();
/// let port = config.get_http_port();
/// ```
pub fn get_config() -> Arc<Config> {
    CONFIG.clone()
}

/// Merges external YAML configuration into default configuration
///
/// This function recursively merges two YAML value trees:
/// - For mappings (objects), it merges keys from external into default
/// - For scalars and sequences, external values replace default values
///
/// # Arguments
///
/// * `default` - The default configuration to merge into (modified in place)
/// * `external` - The external configuration to merge from
fn merge_yaml(default: &mut Value, external: &Value) {
    match (default, external) {
        (Value::Mapping(dmap), Value::Mapping(emap)) => {
            for (k, v) in emap {
                match dmap.get_mut(k) {
                    Some(dv) => merge_yaml(dv, v),
                    None => {
                        dmap.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (d, e) => *d = e.clone(), // pour les scalaires ou séquences, on remplace
    }
}
