//! DEPRECATED: Stub implementation of RadioParadiseSource
//!
//! **⚠️ This module is deprecated and will be removed in a future version.**
//!
//! The orchestration-based RadioParadiseSource has been replaced by
//! `RadioParadiseStreamSource`, which integrates directly with the pmoaudio
//! pipeline for streaming and decoding.
//!
//! ## Migration Guide
//!
//! **Old approach** (deprecated):
//! ```rust,ignore
//! use pmoparadise::RadioParadiseSource;
//! let source = RadioParadiseSource::from_registry(client)?;
//! ```
//!
//! **New approach** (recommended):
//! ```rust,ignore
//! use pmoparadise::RadioParadiseStreamSource;
//! use pmoaudio::pipeline::Node;
//!
//! let stream_source = RadioParadiseStreamSource::new(client, None).await?;
//! let node = Node::from_logic(stream_source);
//! // Use node in pmoaudio pipeline
//! ```
//!
//! This stub implementation is provided only for backward compatibility with
//! existing code (e.g., pmomediaserver) until it can be updated to use
//! RadioParadiseStreamSource.

use crate::client::RadioParadiseClient;
use pmosource::pmodidl::{Container, Item};
use pmosource::{async_trait, BrowseResult, MusicSource, MusicSourceError, Result};
use std::time::SystemTime;

/// Default Radio Paradise image (embedded in binary)
const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/default.webp");

/// DEPRECATED: Stub implementation of RadioParadiseSource
///
/// This is a minimal stub that implements the MusicSource trait with no-op
/// implementations. It exists only to maintain API compatibility during the
/// migration to RadioParadiseStreamSource.
///
/// **Do not use this in new code.** Use `RadioParadiseStreamSource` instead.
#[derive(Clone, Debug)]
pub struct RadioParadiseSource {
    _client: RadioParadiseClient,
}

impl RadioParadiseSource {
    /// DEPRECATED: Create a new RadioParadiseSource from registry
    ///
    /// This method is deprecated and will always return an error indicating
    /// that the orchestration-based source is no longer supported.
    ///
    /// Use `RadioParadiseStreamSource` instead for audio streaming.
    #[cfg(feature = "server")]
    pub fn from_registry(_client: RadioParadiseClient) -> Result<Self> {
        Err(MusicSourceError::SourceUnavailable(
            "RadioParadiseSource is deprecated. Use RadioParadiseStreamSource instead."
                .to_string(),
        ))
    }

    /// DEPRECATED: Create a new RadioParadiseSource from registry with defaults
    ///
    /// This method creates a stub instance that will log deprecation warnings
    /// but allows existing code to compile.
    ///
    /// Use `RadioParadiseStreamSource` instead for audio streaming.
    #[cfg(feature = "server")]
    pub fn from_registry_default(client: RadioParadiseClient) -> Self {
        tracing::warn!(
            "RadioParadiseSource::from_registry_default is deprecated. \
             Use RadioParadiseStreamSource for audio streaming."
        );
        Self { _client: client }
    }

    /// DEPRECATED: Create a new RadioParadiseSource with default settings
    ///
    /// This method is deprecated and only exists for API compatibility.
    pub fn new_default(client: RadioParadiseClient) -> Self {
        tracing::warn!(
            "RadioParadiseSource::new_default is deprecated. \
             Use RadioParadiseStreamSource for audio streaming."
        );
        Self { _client: client }
    }

    /// DEPRECATED: Create a new RadioParadiseSource with cache
    ///
    /// This method is deprecated and only exists for API compatibility.
    pub fn new_with_cache(client: RadioParadiseClient, _cache_size: usize) -> Self {
        tracing::warn!(
            "RadioParadiseSource::new_with_cache is deprecated. \
             Use RadioParadiseStreamSource for audio streaming."
        );
        Self { _client: client }
    }
}

#[async_trait]
impl MusicSource for RadioParadiseSource {
    fn name(&self) -> &str {
        "Radio Paradise (DEPRECATED)"
    }

    fn id(&self) -> &str {
        "radio-paradise-deprecated"
    }

    fn default_image(&self) -> &[u8] {
        DEFAULT_IMAGE
    }

    async fn root_container(&self) -> Result<Container> {
        Ok(Container {
            id: "radio-paradise-deprecated".to_string(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some("0".to_string()),
            searchable: Some("0".to_string()),
            title: "Radio Paradise (DEPRECATED)".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        })
    }

    async fn browse(&self, _object_id: &str) -> Result<BrowseResult> {
        tracing::warn!("RadioParadiseSource::browse called but source is deprecated");
        Ok(BrowseResult::Mixed {
            containers: vec![],
            items: vec![],
        })
    }

    async fn resolve_uri(&self, _object_id: &str) -> Result<String> {
        Err(MusicSourceError::SourceUnavailable(
            "RadioParadiseSource is deprecated. Use RadioParadiseStreamSource instead."
                .to_string(),
        ))
    }

    fn supports_fifo(&self) -> bool {
        false
    }

    async fn append_track(&self, _track: Item) -> Result<()> {
        Err(MusicSourceError::SourceUnavailable(
            "RadioParadiseSource is deprecated and does not support FIFO operations."
                .to_string(),
        ))
    }

    async fn remove_oldest(&self) -> Result<Option<Item>> {
        Ok(None)
    }

    async fn update_id(&self) -> u32 {
        0
    }

    async fn last_change(&self) -> Option<SystemTime> {
        None
    }

    async fn get_items(&self, _offset: usize, _count: usize) -> Result<Vec<Item>> {
        Ok(vec![])
    }
}
