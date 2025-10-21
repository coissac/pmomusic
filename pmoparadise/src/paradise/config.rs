//! Configuration structures for the Radio Paradise orchestration layer.
//!
//! The YAML schema is described in the functional specification.  We expose
//! strongly typed structs with sensible defaults so the rest of the crate can
//! depend on a stable configuration shape irrespective of how the data is
//! loaded (embedded defaults, pmoconfig overrides, tests, etc.).

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Top-level configuration block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioParadiseConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub history: HistoryConfig,
    #[serde(default)]
    pub activity: ActivityConfig,
    #[serde(default)]
    pub polling: PollingConfig,
    #[serde(default)]
    pub stream: StreamConfig,
    #[serde(default)]
    pub api: ApiConfig,
}

impl Default for RadioParadiseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            channels: vec![
                "main".to_string(),
                "mellow".to_string(),
                "rock".to_string(),
                "eclectic".to_string(),
            ],
            cache: CacheConfig::default(),
            history: HistoryConfig::default(),
            activity: ActivityConfig::default(),
            polling: PollingConfig::default(),
            stream: StreamConfig::default(),
            api: ApiConfig::default(),
        }
    }
}

/// Cache related parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "CacheConfig::default_max_blocks")]
    pub max_blocks_remembered: usize,
    #[serde(default = "CacheConfig::default_track_id_bytes")]
    pub track_id_hash_bytes: usize,
}

impl CacheConfig {
    const fn default_max_blocks() -> usize {
        5
    }

    const fn default_track_id_bytes() -> usize {
        512
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_blocks_remembered: Self::default_max_blocks(),
            track_id_hash_bytes: Self::default_track_id_bytes(),
        }
    }
}

/// Persisted history tuning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryConfig {
    #[serde(default = "HistoryConfig::default_max_tracks")]
    pub max_tracks: usize,
    #[serde(default)]
    pub persistence_backend: HistoryBackendKind,
    #[serde(default = "HistoryConfig::default_database_path")]
    pub database_path: String,
}

impl HistoryConfig {
    const fn default_max_tracks() -> usize {
        100
    }

    fn default_database_path() -> String {
        "/var/lib/pmo/paradise_history.db".to_string()
    }
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_tracks: Self::default_max_tracks(),
            persistence_backend: HistoryBackendKind::Sqlite,
            database_path: Self::default_database_path(),
        }
    }
}

impl RadioParadiseConfig {
    pub fn load_from_pmoconfig() -> anyhow::Result<Self> {
        let cfg = pmoconfig::get_config();
        match cfg.get_value(&["sources", "radio_paradise"]) {
            Ok(value) => Ok(serde_yaml::from_value(value).unwrap_or_default()),
            Err(_) => Ok(Self::default()),
        }
    }
}

/// Backend selection for history persistence.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HistoryBackendKind {
    #[default]
    Sqlite,
    Json,
}

/// Activity lifecycle tuning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityConfig {
    #[serde(default = "ActivityConfig::default_cooling_timeout")]
    pub cooling_timeout_seconds: u64,
}

impl ActivityConfig {
    const fn default_cooling_timeout() -> u64 {
        180
    }

    pub fn cooling_timeout(&self) -> Duration {
        Duration::from_secs(self.cooling_timeout_seconds)
    }
}

impl Default for ActivityConfig {
    fn default() -> Self {
        Self {
            cooling_timeout_seconds: Self::default_cooling_timeout(),
        }
    }
}

/// Polling strategy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollingConfig {
    #[serde(default = "PollingConfig::default_interval_high")]
    pub interval_high_buffer: u64,
    #[serde(default = "PollingConfig::default_interval_medium")]
    pub interval_medium_buffer: u64,
    #[serde(default = "PollingConfig::default_interval_low")]
    pub interval_low_buffer: u64,
    #[serde(default)]
    pub backoff_on_error: PollingBackoffConfig,
}

impl PollingConfig {
    const fn default_interval_high() -> u64 {
        120
    }

    const fn default_interval_medium() -> u64 {
        60
    }

    const fn default_interval_low() -> u64 {
        20
    }

    pub fn high_interval(&self) -> Duration {
        Duration::from_secs(self.interval_high_buffer)
    }

    pub fn medium_interval(&self) -> Duration {
        Duration::from_secs(self.interval_medium_buffer)
    }

    pub fn low_interval(&self) -> Duration {
        Duration::from_secs(self.interval_low_buffer)
    }
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            interval_high_buffer: Self::default_interval_high(),
            interval_medium_buffer: Self::default_interval_medium(),
            interval_low_buffer: Self::default_interval_low(),
            backoff_on_error: PollingBackoffConfig::default(),
        }
    }
}

/// Backoff policy for API errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollingBackoffConfig {
    #[serde(default = "PollingBackoffConfig::default_initial")]
    pub initial: u64,
    #[serde(default = "PollingBackoffConfig::default_max")]
    pub max: u64,
    #[serde(default = "PollingBackoffConfig::default_multiplier")]
    pub multiplier: f32,
}

impl PollingBackoffConfig {
    const fn default_initial() -> u64 {
        20
    }

    const fn default_max() -> u64 {
        300
    }

    const fn default_multiplier() -> f32 {
        2.0
    }
}

impl Default for PollingBackoffConfig {
    fn default() -> Self {
        Self {
            initial: Self::default_initial(),
            max: Self::default_max(),
            multiplier: Self::default_multiplier(),
        }
    }
}

/// Streaming pipeline configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    #[serde(default = "StreamConfig::default_metadata_format")]
    pub metadata_format: MetadataFormat,
    #[serde(default)]
    pub enable_gapless: bool,
    #[serde(default = "StreamConfig::default_buffer_size")]
    pub buffer_size_bytes: usize,
}

impl StreamConfig {
    fn default_metadata_format() -> MetadataFormat {
        MetadataFormat::Icy
    }

    const fn default_buffer_size() -> usize {
        64 * 1024
    }
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            metadata_format: MetadataFormat::Icy,
            enable_gapless: true,
            buffer_size_bytes: Self::default_buffer_size(),
        }
    }
}

/// Metadata transport for streaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetadataFormat {
    Icy,
    #[serde(other)]
    None,
}

/// Remote API tuning (timeouts, UA, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(default = "ApiConfig::default_base_url")]
    pub base_url: String,
    #[serde(default = "ApiConfig::default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "ApiConfig::default_user_agent")]
    pub user_agent: String,
}

impl ApiConfig {
    fn default_base_url() -> String {
        "https://api.radioparadise.com".to_string()
    }

    const fn default_timeout() -> u64 {
        30
    }

    fn default_user_agent() -> String {
        "PMO-RadioParadise/1.0".to_string()
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_seconds)
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            base_url: Self::default_base_url(),
            timeout_seconds: Self::default_timeout(),
            user_agent: Self::default_user_agent(),
        }
    }
}
