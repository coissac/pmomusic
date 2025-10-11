use anyhow::{anyhow, Result};
use dirs::home_dir;
use lazy_static::lazy_static;
use pmoutils::guess_local_ip;
use serde_yaml::{Mapping, Value};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tracing::{info, warn};
use uuid::Uuid;

// Configuration par défaut intégrée
const DEFAULT_CONFIG: &str = include_str!("pmomusic.yaml");

lazy_static! {
    static ref CONFIG: Arc<Config> =
        Arc::new(Config::load_config("").expect("Failed to load PMOMusic configuration"));
}

const ENV_CONFIG_FILE: &str = "PMOMUSIC_CONFIG";
const ENV_PREFIX: &str = "PMOMUSIC_CONFIG__";

#[derive(Debug)]
pub struct Config {
    path: String,
    data: Mutex<Value>,
}

// Implémentation manuelle de Clone
impl Clone for Config {
    fn clone(&self) -> Self {
        let data = self.data.lock().unwrap().clone();
        Self {
            path: self.path.clone(),
            data: Mutex::new(data),
        }
    }
}

impl Config {

    pub fn load_config(filename: &str) -> Result<Self> {
        let mut path = filename.to_string();
        let mut data: Option<Vec<u8>> = None;

        let mut default_value: Value = serde_yaml::from_str(DEFAULT_CONFIG)?;

        // Essayer de charger depuis différents emplacements
        if !filename.is_empty() {
            info!(config_file=%path, "Trying to load config");
            data = fs::read(&path).ok();
            if data.is_none() {
                warn!(config_file=%path, "Cannot read config file");
                path.clear();
            }
        }

        if path.is_empty() {
            if let Ok(env_path) = env::var(ENV_CONFIG_FILE) {
                info!(env_var=ENV_CONFIG_FILE, path=%env_path, "Trying to load config from env");
                path = env_path.clone();
                data = fs::read(&path).ok();
                if data.is_none() {
                    warn!(config_file=%path, "Cannot read config file from env var");
                    path.clear();
                }
            }
        }

        if path.is_empty() {
            let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            path = current_dir
                .join(".pmomusic.yml")
                .to_string_lossy()
                .to_string();
            info!(config_file=%path, "Trying to load config file from current directory");
            data = fs::read(&path).ok();
            if data.is_none() {
                warn!(config_file=%path, "Cannot read config file in current dir");
                path.clear();
            }
        }

        if path.is_empty() {
            path = Self::get_home_yml_path();
            info!(config_file=%path, "Trying to load config file from home directory");
            data = fs::read(&path).ok();
            if data.is_none() {
                warn!(config_file=%path, "Cannot read config file in home directory");
                path.clear();
            }
        }

        let yaml_data = if let Some(d) = data {
            d
        } else {
            info!("Using default embedded config");
            DEFAULT_CONFIG.as_bytes().to_vec()
        };


        let external_value: Value = serde_yaml::from_slice(&yaml_data)?;
        merge_yaml(&mut default_value, &external_value);
        let mut config_value  = Self::lower_keys_value(default_value);

        Self::apply_env_overrides(&mut config_value);

        if path.is_empty() || !Self::is_writable(&path) {
            let candidates = [
                filename.to_string(),
                env::var(ENV_CONFIG_FILE).unwrap_or_default(),
                ".pmomusic.yml".to_string(),
                Self::get_home_yml_path(),
            ];
            for candidate in candidates.iter().filter(|c| !c.is_empty()) {
                if Self::is_writable(candidate) {
                    path = candidate.clone();
                    break;
                }
            }
        }

        if path.is_empty() {
            return Err(anyhow!("Cannot find a place to store config file"));
        }

        info!(config_file=%path, "Config file will be stored here");

        let config = Config {
            path,
            data: Mutex::new(config_value),
        };
        config.save()?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let data = self.data.lock().unwrap();
        let yaml = serde_yaml::to_string(&*data)?;
        fs::write(&self.path, yaml)?;
        Ok(())
    }

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

    fn get_home_yml_path() -> String {
        home_dir()
            .map(|p| p.join(".pmomusic.yml"))
            .unwrap_or_else(|| PathBuf::from("."))
            .to_string_lossy()
            .to_string()
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

    fn is_writable(path: &str) -> bool {
        let path = Path::new(path);
        if let Some(parent) = path.parent() {
            fs::metadata(parent)
                .map(|m| !m.permissions().readonly())
                .unwrap_or(false)
        } else {
            false
        }
    }

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

    pub fn get_http_port(&self) -> u16 {
        match self.get_value(&["host", "http_port"]) {
            Ok(Value::Number(n)) if n.is_i64() => n.as_i64().unwrap() as u16,
            Ok(Value::String(s)) => match s.parse::<u16>() {
                Ok(port) => port,
                Err(_) => {
                    tracing::warn!("Invalid HTTP port '{}', using default 8080", s);
                    8080
                }
            },
            Ok(_) => {
                tracing::warn!("HTTP port not a number or string, using default 8080");
                8080
            }
            Err(err) => {
                tracing::warn!("Failed to get HTTP port: {}, using default 8080", err);
                8080
            }
        }
    }

    pub fn get_device_udn(&self, devtype: &str, name: &str) -> Result<String> {
        let path = &["devices", devtype, name, "udn"];
        match self.get_value(path) {
            Ok(Value::String(udn)) => Ok(udn),
            _ => {
                let new_udn = Uuid::new_v4().to_string();
                self.set_value(path, Value::String(new_udn.clone()))?;
                Ok(new_udn)
            }
        }
    }

    pub fn get_cover_cache_dir(&self) -> Result<String> {
        match self.get_value(&["host", "cover_cache", "directory"])? {
            Value::String(s) => Ok(s),
            _ => Ok("./.pmomusic_covers".to_string()),
        }
    }

    pub fn get_cover_cache_size(&self) -> Result<usize> {
        match self.get_value(&["host", "cover_cache", "size"])? {
            Value::Number(n) if n.is_i64() => Ok(n.as_i64().unwrap() as usize),
            Value::Number(n) if n.is_u64() => Ok(n.as_u64().unwrap() as usize),
            _ => Ok(2000),
        }
    }

    /// Récupère le nom d'utilisateur Qobuz depuis la configuration
    pub fn get_qobuz_username(&self) -> Result<String> {
        match self.get_value(&["accounts", "qobuz", "username"])? {
            Value::String(s) => Ok(s),
            _ => Err(anyhow!("Qobuz username not configured")),
        }
    }

    /// Définit le nom d'utilisateur Qobuz dans la configuration
    pub fn set_qobuz_username(&self, username: &str) -> Result<()> {
        self.set_value(&["accounts", "qobuz", "username"], Value::String(username.to_string()))
    }

    /// Récupère le mot de passe Qobuz depuis la configuration
    pub fn get_qobuz_password(&self) -> Result<String> {
        match self.get_value(&["accounts", "qobuz", "password"])? {
            Value::String(s) => Ok(s),
            _ => Err(anyhow!("Qobuz password not configured")),
        }
    }

    /// Définit le mot de passe Qobuz dans la configuration
    pub fn set_qobuz_password(&self, password: &str) -> Result<()> {
        self.set_value(&["accounts", "qobuz", "password"], Value::String(password.to_string()))
    }

    /// Récupère les credentials Qobuz (username + password) depuis la configuration
    pub fn get_qobuz_credentials(&self) -> Result<(String, String)> {
        let username = self.get_qobuz_username()?;
        let password = self.get_qobuz_password()?;
        Ok((username, password))
    }
}

/// Retourne l'instance globale
pub fn get_config() -> Arc<Config> {
    CONFIG.clone()
}

fn merge_yaml(default: &mut Value, external: &Value) {
    match (default, external) {
        (Value::Mapping(dmap), Value::Mapping(emap)) => {
            for (k, v) in emap {
                match dmap.get_mut(k) {
                    Some(dv) => merge_yaml(dv, v),
                    None => { dmap.insert(k.clone(), v.clone()); }
                }
            }
        }
        (d, e) => *d = e.clone(), // pour les scalaires ou séquences, on remplace
    }
}
