//! Minimal metadata abstraction shared between PMO crates.
//!
//! This crate provides a lightweight, async-friendly trait [`TrackMetadata`] for managing
//! audio track metadata across different implementations. It supports both in-memory
//! storage and database-backed implementations.
//!
//! # Features
//!
//! - **Async API**: All operations are asynchronous to support database backends
//! - **Optional fields**: Implementations only need to override supported fields
//! - **Error handling**: Distinguishes between transient errors (NotImplemented, ReadOnly)
//!   and backend errors that should be propagated
//! - **Metadata copying**: Helper function to copy metadata between implementations
//!
//! # Examples
//!
//! ```rust
//! use pmometadata::{TrackMetadata, MemoryTrackMetadata};
//! use std::time::Duration;
//!
//! # tokio_test::block_on(async {
//! let mut metadata = MemoryTrackMetadata::new();
//!
//! // Set some metadata
//! metadata.set_title(Some("My Song".to_string())).await.unwrap();
//! metadata.set_artist(Some("Artist Name".to_string())).await.unwrap();
//! metadata.set_duration(Some(Duration::from_secs(180))).await.unwrap();
//!
//! // Retrieve metadata
//! assert_eq!(metadata.get_title().await.unwrap(), Some("My Song".to_string()));
//! # });
//! ```
#![allow(async_fn_in_trait)]

use async_trait::async_trait;
use std::sync::Arc;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;

/// Helper macro for copying a single metadata field.
macro_rules! copy_a_metadata {
    ($src:ident, $dest:ident, $key:ident) => {
        ::paste::paste! {
            match $src.[<get_ $key>]().await {
                Ok(Some(value)) => {
                    match $dest.write().await.[<set_ $key>](Some(value)).await {
                        Ok(_) => {},
                        Err(e) if e.is_transient() => {
                            // Transient error, try setting to None instead
                            match $dest.write().await.[<set_ $key>](None).await {
                                Ok(_) => {},
                                Err(e2) if e2.is_transient() => {},
                                Err(e2) => return Err(e2),
                            }
                        }
                        Err(e) => return Err(e),
                    }
                },
                Ok(None) => {
                    match $dest.write().await.[<set_ $key>](None).await {
                        Ok(_) => {},
                        Err(e) if e.is_transient() => {},
                        Err(e) => return Err(e),
                    }
                }
                Err(e) if e.is_transient() => {
                    match $dest.write().await.[<set_ $key>](None).await {
                        Ok(_) => {},
                        Err(e2) if e2.is_transient() => {},
                        Err(e2) => return Err(e2),
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
    };
}

/// Helper macro for copying multiple metadata fields.
macro_rules! copy_metadata {
    ($src:ident, $dest:ident, $( $key:ident ),*) => {
        $(copy_a_metadata!($src, $dest, $key);)*
    };
}

/// Convenience alias for metadata operations that can fail or return no value.
///
/// Returns `Ok(Some(T))` when a value is present, `Ok(None)` when the field is empty,
/// or `Err(MetadataError)` when an error occurs.
pub type MetadataResult<T> = Result<Option<T>, MetadataError>;

/// Errors that can occur when manipulating metadata.
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    /// The metadata field is not implemented by this provider.
    ///
    /// This is a transient error that indicates the implementation doesn't support
    /// this particular field. When copying metadata, these errors are ignored.
    #[error("metadata field is not implemented")]
    NotImplemented,

    /// The metadata field is read-only and cannot be modified.
    ///
    /// This is a transient error. When copying metadata, these errors are ignored.
    #[error("metadata field is read-only")]
    ReadOnly,

    /// An error occurred in the backend (e.g., database error).
    ///
    /// These errors are non-transient and should be propagated to the caller.
    #[error("backend error: {0}")]
    Backend(String),
}

impl MetadataError {
    /// Returns `true` if this error is transient and can be safely ignored.
    ///
    /// Transient errors (`NotImplemented`, `ReadOnly`) indicate that the operation
    /// is not supported but is not a critical failure. When copying metadata,
    /// transient errors result in setting the destination field to `None`.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            MetadataError::NotImplemented | MetadataError::ReadOnly
        )
    }
}

/// Trait implemented by metadata providers.
///
/// This trait provides a uniform interface for accessing and modifying audio track metadata.
/// All methods have default implementations that return `NotImplemented`, so implementations
/// only need to override the fields they support.
///
/// # Implementing the trait
///
/// Implementations should:
/// - Override `get_*` methods for readable fields
/// - Override `set_*` methods for writable fields
/// - Call `touch()` in setters to update the modification timestamp
/// - Return `Ok(Some(value))` for successful operations
/// - Return `Ok(None)` when a field is empty
/// - Return `Err(NotImplemented)` for unsupported fields
/// - Return `Err(ReadOnly)` for read-only fields
/// - Return `Err(Backend(msg))` for backend errors
///
/// # Thread safety
///
/// All implementations must be `Send + Sync` to work in async contexts.
#[async_trait]
pub trait TrackMetadata: Send + Sync {
    async fn get_title(&self) -> MetadataResult<String> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_title(&mut self, _value: Option<String>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_artist(&self) -> MetadataResult<String> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_artist(&mut self, _value: Option<String>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_album(&self) -> MetadataResult<String> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_album(&mut self, _value: Option<String>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_year(&self) -> MetadataResult<u32> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_year(&mut self, _value: Option<u32>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_duration(&self) -> MetadataResult<Duration> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_duration(&mut self, _value: Option<Duration>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_sample_rate(&self) -> MetadataResult<u32> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_sample_rate(&mut self, _value: Option<u32>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_total_samples(&self) -> MetadataResult<u64> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_total_samples(&mut self, _value: Option<u64>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_bits_per_sample(&self) -> MetadataResult<u8> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_bits_per_sample(&mut self, _value: Option<u8>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_track_id(&self) -> MetadataResult<String> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_track_id(&mut self, _value: Option<String>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_channel_id(&self) -> MetadataResult<String> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_channel_id(&mut self, _value: Option<String>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_event(&self) -> MetadataResult<String> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_event(&mut self, _value: Option<String>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_rating(&self) -> MetadataResult<f32> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_rating(&mut self, _value: Option<f32>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_cover_url(&self) -> MetadataResult<String> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_cover_url(&mut self, _value: Option<String>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_cover_pk(&self) -> MetadataResult<String> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_cover_pk(&mut self, _value: Option<String>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_extra(&self) -> MetadataResult<HashMap<String, String>> {
        Err(MetadataError::NotImplemented)
    }

    async fn set_extra(&mut self, _value: Option<HashMap<String, String>>) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }

    async fn get_updated_at(&self) -> MetadataResult<SystemTime> {
        Err(MetadataError::NotImplemented)
    }

    async fn touch(&mut self) -> MetadataResult<()> {
        Err(MetadataError::NotImplemented)
    }
}

/// Copies all available metadata from one implementation to another.
///
/// This function reads all metadata fields from the source and attempts to write them
/// to the destination. It handles errors intelligently:
///
/// - **Transient errors** (`NotImplemented`, `ReadOnly`): The field is set to `None` in the destination
/// - **Backend errors**: The error is propagated to the caller
/// - **Success**: The value is copied from source to destination
///
/// After all fields are copied, the destination's `touch()` method is called to update
/// its modification timestamp.
///
/// # Arguments
///
/// * `src` - Source metadata wrapped in `Arc<RwLock<_>>`
/// * `dest` - Destination metadata wrapped in `Arc<RwLock<_>>`
///
/// # Errors
///
/// Returns an error if:
/// - The destination returns a backend error when setting a field
/// - The destination's `touch()` method returns an error
///
/// # Examples
///
/// ```rust
/// use pmometadata::{TrackMetadata, MemoryTrackMetadata, copy_metadata_into};
/// use std::sync::Arc;
/// use tokio::sync::RwLock;
///
/// # tokio_test::block_on(async {
/// let mut src = MemoryTrackMetadata::new();
/// src.set_title(Some("My Song".to_string())).await.unwrap();
/// src.set_artist(Some("Artist".to_string())).await.unwrap();
///
/// let dest = MemoryTrackMetadata::new();
///
/// let src_lock = Arc::new(RwLock::new(src));
/// let dest_lock = Arc::new(RwLock::new(dest));
///
/// copy_metadata_into(&src_lock, &dest_lock).await.unwrap();
///
/// assert_eq!(
///     dest_lock.read().await.get_title().await.unwrap(),
///     Some("My Song".to_string())
/// );
/// # });
/// ```
pub async fn copy_metadata_into<S, D>(
    src: &Arc<RwLock<S>>,
    dest: &Arc<RwLock<D>>,
) -> Result<(), MetadataError>
where
    S: TrackMetadata + ?Sized,
    D: TrackMetadata + ?Sized,
{
    let src_guard = src.read().await;

    copy_metadata!(
        src_guard, dest, title, artist, album, year, duration, sample_rate, total_samples,
        bits_per_sample, track_id, channel_id, event, rating, cover_url, cover_pk, extra
    );

    // Try to update the timestamp, but ignore transient errors
    match dest.write().await.touch().await {
        Ok(_) => Ok(()),
        Err(e) if e.is_transient() => Ok(()),
        Err(e) => Err(e),
    }
}

/// In-memory metadata implementation with full read/write support.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryTrackMetadata {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    year: Option<u32>,
    duration: Option<Duration>,
    sample_rate: Option<u32>,
    total_samples: Option<u64>,
    bits_per_sample: Option<u8>,
    track_id: Option<String>,
    channel_id: Option<String>,
    event: Option<String>,
    rating: Option<f32>,
    cover_url: Option<String>,
    cover_pk: Option<String>,
    updated_at: Option<SystemTime>,
    extra: Option<HashMap<String, String>>,
}

impl MemoryTrackMetadata {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TrackMetadata for MemoryTrackMetadata {
    async fn get_title(&self) -> MetadataResult<String> {
        Ok(self.title.clone())
    }

    async fn set_title(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.title = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_artist(&self) -> MetadataResult<String> {
        Ok(self.artist.clone())
    }

    async fn set_artist(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.artist = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_album(&self) -> MetadataResult<String> {
        Ok(self.album.clone())
    }

    async fn set_album(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.album = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_year(&self) -> MetadataResult<u32> {
        Ok(self.year)
    }

    async fn set_year(&mut self, value: Option<u32>) -> MetadataResult<()> {
        self.year = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_duration(&self) -> MetadataResult<Duration> {
        Ok(self.duration)
    }

    async fn set_duration(&mut self, value: Option<Duration>) -> MetadataResult<()> {
        self.duration = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_sample_rate(&self) -> MetadataResult<u32> {
        Ok(self.sample_rate)
    }

    async fn set_sample_rate(&mut self, value: Option<u32>) -> MetadataResult<()> {
        self.sample_rate = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_total_samples(&self) -> MetadataResult<u64> {
        Ok(self.total_samples)
    }

    async fn set_total_samples(&mut self, value: Option<u64>) -> MetadataResult<()> {
        self.total_samples = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_bits_per_sample(&self) -> MetadataResult<u8> {
        Ok(self.bits_per_sample)
    }

    async fn set_bits_per_sample(&mut self, value: Option<u8>) -> MetadataResult<()> {
        self.bits_per_sample = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_track_id(&self) -> MetadataResult<String> {
        Ok(self.track_id.clone())
    }

    async fn set_track_id(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.track_id = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_channel_id(&self) -> MetadataResult<String> {
        Ok(self.channel_id.clone())
    }

    async fn set_channel_id(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.channel_id = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_event(&self) -> MetadataResult<String> {
        Ok(self.event.clone())
    }

    async fn set_event(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.event = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_rating(&self) -> MetadataResult<f32> {
        Ok(self.rating)
    }

    async fn set_rating(&mut self, value: Option<f32>) -> MetadataResult<()> {
        self.rating = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_cover_url(&self) -> MetadataResult<String> {
        Ok(self.cover_url.clone())
    }

    async fn set_cover_url(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.cover_url = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_cover_pk(&self) -> MetadataResult<String> {
        Ok(self.cover_pk.clone())
    }

    async fn set_cover_pk(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.cover_pk = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_extra(&self) -> MetadataResult<HashMap<String, String>> {
        Ok(self.extra.clone())
    }

    async fn set_extra(&mut self, value: Option<HashMap<String, String>>) -> MetadataResult<()> {
        self.extra = value;
        self.touch().await?;
        Ok(Some(()))
    }

    async fn get_updated_at(&self) -> MetadataResult<SystemTime> {
        Ok(self.updated_at)
    }

    async fn touch(&mut self) -> MetadataResult<()> {
        self.updated_at = Some(SystemTime::now());
        Ok(Some(()))
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_metadata_new() {
        let metadata = MemoryTrackMetadata::new();

        assert_eq!(metadata.get_title().await.unwrap(), None);
        assert_eq!(metadata.get_artist().await.unwrap(), None);
        assert_eq!(metadata.get_album().await.unwrap(), None);
        assert_eq!(metadata.get_year().await.unwrap(), None);
        assert_eq!(metadata.get_duration().await.unwrap(), None);
        assert_eq!(metadata.get_sample_rate().await.unwrap(), None);
        assert_eq!(metadata.get_total_samples().await.unwrap(), None);
        assert_eq!(metadata.get_bits_per_sample().await.unwrap(), None);
        assert_eq!(metadata.get_updated_at().await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_memory_metadata_set_get_title() {
        let mut metadata = MemoryTrackMetadata::new();

        let result = metadata.set_title(Some("Test Title".to_string())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(()));

        let title = metadata.get_title().await.unwrap();
        assert_eq!(title, Some("Test Title".to_string()));
    }

    #[tokio::test]
    async fn test_memory_metadata_set_get_artist() {
        let mut metadata = MemoryTrackMetadata::new();

        metadata
            .set_artist(Some("Artist Name".to_string()))
            .await
            .unwrap();
        assert_eq!(
            metadata.get_artist().await.unwrap(),
            Some("Artist Name".to_string())
        );
    }

    #[tokio::test]
    async fn test_memory_metadata_set_get_album() {
        let mut metadata = MemoryTrackMetadata::new();

        metadata
            .set_album(Some("Album Name".to_string()))
            .await
            .unwrap();
        assert_eq!(
            metadata.get_album().await.unwrap(),
            Some("Album Name".to_string())
        );
    }

    #[tokio::test]
    async fn test_memory_metadata_set_get_year() {
        let mut metadata = MemoryTrackMetadata::new();

        metadata.set_year(Some(2024)).await.unwrap();
        assert_eq!(metadata.get_year().await.unwrap(), Some(2024));
    }

    #[tokio::test]
    async fn test_memory_metadata_set_get_duration() {
        let mut metadata = MemoryTrackMetadata::new();
        let duration = Duration::from_secs(180);

        metadata.set_duration(Some(duration)).await.unwrap();
        assert_eq!(metadata.get_duration().await.unwrap(), Some(duration));
    }

    #[tokio::test]
    async fn test_memory_metadata_set_get_rating() {
        let mut metadata = MemoryTrackMetadata::new();

        metadata.set_rating(Some(4.5)).await.unwrap();
        assert_eq!(metadata.get_rating().await.unwrap(), Some(4.5));
    }

    #[tokio::test]
    async fn test_memory_metadata_set_get_track_id() {
        let mut metadata = MemoryTrackMetadata::new();

        metadata
            .set_track_id(Some("track123".to_string()))
            .await
            .unwrap();
        assert_eq!(
            metadata.get_track_id().await.unwrap(),
            Some("track123".to_string())
        );
    }

    #[tokio::test]
    async fn test_memory_metadata_set_get_channel_id() {
        let mut metadata = MemoryTrackMetadata::new();

        metadata
            .set_channel_id(Some("channel456".to_string()))
            .await
            .unwrap();
        assert_eq!(
            metadata.get_channel_id().await.unwrap(),
            Some("channel456".to_string())
        );
    }

    #[tokio::test]
    async fn test_memory_metadata_set_get_event() {
        let mut metadata = MemoryTrackMetadata::new();

        metadata
            .set_event(Some("event789".to_string()))
            .await
            .unwrap();
        assert_eq!(
            metadata.get_event().await.unwrap(),
            Some("event789".to_string())
        );
    }

    #[tokio::test]
    async fn test_memory_metadata_set_get_cover_url() {
        let mut metadata = MemoryTrackMetadata::new();

        metadata
            .set_cover_url(Some("https://example.com/cover.jpg".to_string()))
            .await
            .unwrap();
        assert_eq!(
            metadata.get_cover_url().await.unwrap(),
            Some("https://example.com/cover.jpg".to_string())
        );
    }

    #[tokio::test]
    async fn test_memory_metadata_set_get_cover_pk() {
        let mut metadata = MemoryTrackMetadata::new();

        metadata
            .set_cover_pk(Some("pk123".to_string()))
            .await
            .unwrap();
        assert_eq!(
            metadata.get_cover_pk().await.unwrap(),
            Some("pk123".to_string())
        );
    }

    #[tokio::test]
    async fn test_memory_metadata_set_get_extra() {
        let mut metadata = MemoryTrackMetadata::new();
        let mut extra = HashMap::new();
        extra.insert("key1".to_string(), "value1".to_string());
        extra.insert("key2".to_string(), "value2".to_string());

        metadata.set_extra(Some(extra.clone())).await.unwrap();
        assert_eq!(metadata.get_extra().await.unwrap(), Some(extra));
    }

    #[tokio::test]
    async fn test_memory_metadata_set_none() {
        let mut metadata = MemoryTrackMetadata::new();

        metadata.set_title(Some("Title".to_string())).await.unwrap();
        assert_eq!(
            metadata.get_title().await.unwrap(),
            Some("Title".to_string())
        );

        metadata.set_title(None).await.unwrap();
        assert_eq!(metadata.get_title().await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_memory_metadata_touch() {
        let mut metadata = MemoryTrackMetadata::new();

        assert_eq!(metadata.get_updated_at().await.unwrap(), None);

        metadata.touch().await.unwrap();
        let updated_at = metadata.get_updated_at().await.unwrap();
        assert!(updated_at.is_some());
    }

    #[tokio::test]
    async fn test_memory_metadata_touch_on_set() {
        let mut metadata = MemoryTrackMetadata::new();

        assert_eq!(metadata.get_updated_at().await.unwrap(), None);

        metadata.set_title(Some("Title".to_string())).await.unwrap();
        let updated_at = metadata.get_updated_at().await.unwrap();
        assert!(updated_at.is_some());
    }

    #[tokio::test]
    async fn test_metadata_error_is_transient() {
        assert!(MetadataError::NotImplemented.is_transient());
        assert!(MetadataError::ReadOnly.is_transient());
        assert!(!MetadataError::Backend("error".to_string()).is_transient());
    }

    #[tokio::test]
    async fn test_copy_metadata_into_basic() {
        let mut src = MemoryTrackMetadata::new();
        src.set_title(Some("Source Title".to_string()))
            .await
            .unwrap();
        src.set_artist(Some("Source Artist".to_string()))
            .await
            .unwrap();
        src.set_year(Some(2024)).await.unwrap();

        let dest = MemoryTrackMetadata::new();

        let src_lock = Arc::new(RwLock::new(src));
        let dest_lock = Arc::new(RwLock::new(dest));

        copy_metadata_into(&src_lock, &dest_lock).await.unwrap();

        let dest_guard = dest_lock.read().await;
        assert_eq!(
            dest_guard.get_title().await.unwrap(),
            Some("Source Title".to_string())
        );
        assert_eq!(
            dest_guard.get_artist().await.unwrap(),
            Some("Source Artist".to_string())
        );
        assert_eq!(dest_guard.get_year().await.unwrap(), Some(2024));
    }

    #[tokio::test]
    async fn test_copy_metadata_into_partial() {
        let mut src = MemoryTrackMetadata::new();
        src.set_title(Some("Title".to_string())).await.unwrap();
        // artist is None

        let dest = MemoryTrackMetadata::new();

        let src_lock = Arc::new(RwLock::new(src));
        let dest_lock = Arc::new(RwLock::new(dest));

        copy_metadata_into(&src_lock, &dest_lock).await.unwrap();

        let dest_guard = dest_lock.read().await;
        assert_eq!(
            dest_guard.get_title().await.unwrap(),
            Some("Title".to_string())
        );
        assert_eq!(dest_guard.get_artist().await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_copy_metadata_into_updates_timestamp() {
        let src = MemoryTrackMetadata::new();
        let dest = MemoryTrackMetadata::new();

        let src_lock = Arc::new(RwLock::new(src));
        let dest_lock = Arc::new(RwLock::new(dest));

        let before = dest_lock.read().await.get_updated_at().await.unwrap();
        assert_eq!(before, None);

        copy_metadata_into(&src_lock, &dest_lock).await.unwrap();

        let after = dest_lock.read().await.get_updated_at().await.unwrap();
        assert!(after.is_some());
    }

    #[tokio::test]
    async fn test_copy_metadata_into_all_fields() {
        let mut src = MemoryTrackMetadata::new();
        src.set_title(Some("Title".to_string())).await.unwrap();
        src.set_artist(Some("Artist".to_string())).await.unwrap();
        src.set_album(Some("Album".to_string())).await.unwrap();
        src.set_year(Some(2024)).await.unwrap();
        src.set_duration(Some(Duration::from_secs(180)))
            .await
            .unwrap();
        src.set_track_id(Some("track123".to_string()))
            .await
            .unwrap();
        src.set_channel_id(Some("channel456".to_string()))
            .await
            .unwrap();
        src.set_event(Some("event789".to_string())).await.unwrap();
        src.set_rating(Some(4.5)).await.unwrap();
        src.set_cover_url(Some("https://example.com/cover.jpg".to_string()))
            .await
            .unwrap();
        src.set_cover_pk(Some("pk123".to_string())).await.unwrap();

        let mut extra = HashMap::new();
        extra.insert("key".to_string(), "value".to_string());
        src.set_extra(Some(extra.clone())).await.unwrap();

        let dest = MemoryTrackMetadata::new();

        let src_lock = Arc::new(RwLock::new(src));
        let dest_lock = Arc::new(RwLock::new(dest));

        copy_metadata_into(&src_lock, &dest_lock).await.unwrap();

        let dest_guard = dest_lock.read().await;
        assert_eq!(
            dest_guard.get_title().await.unwrap(),
            Some("Title".to_string())
        );
        assert_eq!(
            dest_guard.get_artist().await.unwrap(),
            Some("Artist".to_string())
        );
        assert_eq!(
            dest_guard.get_album().await.unwrap(),
            Some("Album".to_string())
        );
        assert_eq!(dest_guard.get_year().await.unwrap(), Some(2024));
        assert_eq!(
            dest_guard.get_duration().await.unwrap(),
            Some(Duration::from_secs(180))
        );
        assert_eq!(
            dest_guard.get_track_id().await.unwrap(),
            Some("track123".to_string())
        );
        assert_eq!(
            dest_guard.get_channel_id().await.unwrap(),
            Some("channel456".to_string())
        );
        assert_eq!(
            dest_guard.get_event().await.unwrap(),
            Some("event789".to_string())
        );
        assert_eq!(dest_guard.get_rating().await.unwrap(), Some(4.5));
        assert_eq!(
            dest_guard.get_cover_url().await.unwrap(),
            Some("https://example.com/cover.jpg".to_string())
        );
        assert_eq!(
            dest_guard.get_cover_pk().await.unwrap(),
            Some("pk123".to_string())
        );
        assert_eq!(dest_guard.get_extra().await.unwrap(), Some(extra));
    }

    #[tokio::test]
    async fn test_copy_metadata_handles_errors() {
        // Test that copy works even when some fields can't be set
        let mut src = MemoryTrackMetadata::new();
        src.set_title(Some("Title".to_string())).await.unwrap();
        src.set_artist(Some("Artist".to_string())).await.unwrap();

        let dest = MemoryTrackMetadata::new();

        let src_lock = Arc::new(RwLock::new(src));
        let dest_lock = Arc::new(RwLock::new(dest));

        // Should succeed
        let result = copy_metadata_into(&src_lock, &dest_lock).await;
        assert!(result.is_ok());

        // Verify the data was copied
        let dest_guard = dest_lock.read().await;
        assert_eq!(
            dest_guard.get_title().await.unwrap(),
            Some("Title".to_string())
        );
        assert_eq!(
            dest_guard.get_artist().await.unwrap(),
            Some("Artist".to_string())
        );
    }
}
