//! Minimal metadata abstraction shared between PMO crates.
#![allow(async_fn_in_trait)]

use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;
use std::sync::Arc;
use async_trait::async_trait;

/// Convenience alias for metadata operations that can fail or return no value.
pub type MetadataResult<T> = Result<Option<T>, MetadataError>;

/// Errors that can occur when manipulating metadata.
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("metadata field is not implemented")]
    NotImplemented,
    #[error("metadata field is read-only")]
    ReadOnly,
    #[error("backend error: {0}")]
    Backend(String),
}

impl MetadataError {
    fn is_transient(&self) -> bool {
        matches!(
            self,
            MetadataError::NotImplemented | MetadataError::ReadOnly
        )
    }
}

/// Trait implemented by metadata providers.
///
/// Only the fields supported by the implementation have to be overridden.
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

pub async fn copy_metadata_into<S, D>(
    src: &Arc<RwLock<S>>,
    dest: &Arc<RwLock<D>>,
) -> Result<(), MetadataError>
where
    S: TrackMetadata + ?Sized,
    D: TrackMetadata + ?Sized,
{
    let src_guard = src.read().await;
    
    // Expansion de copy_metadata! pour toutes les propriétés
    match src_guard.get_title().await {
        Ok(Some(value)) => {
            dest.write().await.set_title(Some(value)).await?;
        },
        Ok(None) | Err(_) => {
            dest.write().await.set_title(None).await.ok();
        }
    }

    match src_guard.get_artist().await {
        Ok(Some(value)) => {
            dest.write().await.set_artist(Some(value)).await?;
        },
        Ok(None) | Err(_) => {
            dest.write().await.set_artist(None).await.ok();
        }
    }

    match src_guard.get_album().await {
        Ok(Some(value)) => {
            dest.write().await.set_album(Some(value)).await?;
        },
        Ok(None) | Err(_) => {
            dest.write().await.set_album(None).await.ok();
        }
    }

    match src_guard.get_year().await {
        Ok(Some(value)) => {
            dest.write().await.set_year(Some(value)).await?;
        },
        Ok(None) | Err(_) => {
            dest.write().await.set_year(None).await.ok();
        }
    }

    match src_guard.get_duration().await {
        Ok(Some(value)) => {
            dest.write().await.set_duration(Some(value)).await?;
        },
        Ok(None) | Err(_) => {
            dest.write().await.set_duration(None).await.ok();
        }
    }

    match src_guard.get_track_id().await {
        Ok(Some(value)) => {
            dest.write().await.set_track_id(Some(value)).await?;
        },
        Ok(None) | Err(_) => {
            dest.write().await.set_track_id(None).await.ok();
        }
    }

    match src_guard.get_channel_id().await {
        Ok(Some(value)) => {
            dest.write().await.set_channel_id(Some(value)).await?;
        },
        Ok(None) | Err(_) => {
            dest.write().await.set_channel_id(None).await.ok();
        }
    }

    match src_guard.get_event().await {
        Ok(Some(value)) => {
            dest.write().await.set_event(Some(value)).await?;
        },
        Ok(None) | Err(_) => {
            dest.write().await.set_event(None).await.ok();
        }
    }

    match src_guard.get_rating().await {
        Ok(Some(value)) => {
            dest.write().await.set_rating(Some(value)).await?;
        },
        Ok(None) | Err(_) => {
            dest.write().await.set_rating(None).await.ok();
        }
    }

    match src_guard.get_cover_url().await {
        Ok(Some(value)) => {
            dest.write().await.set_cover_url(Some(value)).await?;
        },
        Ok(None) | Err(_) => {
            dest.write().await.set_cover_url(None).await.ok();
        }
    }

    match src_guard.get_cover_pk().await {
        Ok(Some(value)) => {
            dest.write().await.set_cover_pk(Some(value)).await?;
        },
        Ok(None) | Err(_) => {
            dest.write().await.set_cover_pk(None).await.ok();
        }
    }

    match src_guard.get_extra().await {
        Ok(Some(value)) => {
            dest.write().await.set_extra(Some(value)).await?;
        },
        Ok(None) | Err(_) => {
            dest.write().await.set_extra(None).await.ok();
        }
    }

    dest.write().await.touch().await?;
    Ok(())
}

/// In-memory metadata implementation with full read/write support.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryTrackMetadata {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    year: Option<u32>,
    duration: Option<Duration>,
    elapsed: Option<Duration>,
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
