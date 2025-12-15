use crate::client::QobuzClient;
use crate::models::Track;
use anyhow::{anyhow, Result};
use pmoaudiocache::AudioMetadata;
use pmocache::lazy::LazyProvider;
use serde_json::Value;
use std::sync::Arc;

pub struct QobuzLazyProvider {
    client: Arc<QobuzClient>,
}

impl QobuzLazyProvider {
    pub fn new(client: Arc<QobuzClient>) -> Self {
        Self { client }
    }

    fn track_id_from_lazy<'a>(&self, lazy_pk: &'a str) -> Result<&'a str> {
        match lazy_pk.split_once(':') {
            Some((prefix, value)) if prefix.eq_ignore_ascii_case("QOBUZ") => Ok(value),
            _ => Err(anyhow!("Invalid Qobuz lazy pk {}", lazy_pk)),
        }
    }

    async fn fetch_track(&self, lazy_pk: &str) -> Result<Track> {
        let track_id = self.track_id_from_lazy(lazy_pk)?;
        self.client
            .get_track(track_id)
            .await
            .map_err(|e| anyhow!("Failed to fetch track {}: {}", track_id, e))
    }

    fn build_metadata(track: &Track) -> AudioMetadata {
        AudioMetadata {
            title: Some(track.title.clone()),
            artist: track.performer.as_ref().map(|p| p.name.clone()),
            album: track.album.as_ref().map(|a| a.title.clone()),
            duration_secs: Some(track.duration as u64),
            year: track.album.as_ref().and_then(|a| {
                a.release_date
                    .as_ref()
                    .and_then(|d| d.split('-').next()?.parse().ok())
            }),
            track_number: Some(track.track_number),
            track_total: track.album.as_ref().and_then(|a| a.tracks_count),
            disc_number: Some(track.media_number),
            disc_total: None,
            genre: track.album.as_ref().and_then(|a| {
                if !a.genres.is_empty() {
                    Some(a.genres.join(", "))
                } else {
                    None
                }
            }),
            sample_rate: track.sample_rate,
            channels: track.channels,
            bitrate: None,
            conversion: None,
        }
    }
}

#[async_trait::async_trait]
impl LazyProvider for QobuzLazyProvider {
    fn lazy_prefix(&self) -> &'static str {
        "QOBUZ"
    }

    async fn get_url(&self, lazy_pk: &str) -> Result<String> {
        let track_id = self.track_id_from_lazy(lazy_pk)?;
        self.client
            .get_stream_url(track_id)
            .await
            .map_err(|e| anyhow!("Failed to resolve stream URL for {}: {}", track_id, e))
    }

    async fn metadata(&self, lazy_pk: &str) -> Result<Option<Value>> {
        let track = self.fetch_track(lazy_pk).await?;
        let metadata = Self::build_metadata(&track);
        let value = serde_json::to_value(metadata)?;
        Ok(Some(value))
    }

    async fn cover_url(&self, lazy_pk: &str) -> Result<Option<String>> {
        let track = self.fetch_track(lazy_pk).await?;
        Ok(track
            .album
            .and_then(|a| a.image)
            .and_then(|url| if url.is_empty() { None } else { Some(url) }))
    }
}
