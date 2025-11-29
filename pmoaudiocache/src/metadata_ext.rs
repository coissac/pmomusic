//! Extension trait pour accéder aux métadonnées audio via `TrackMetadata`
//!
//! Cette couche est désormais un mince wrapper qui délègue au nœud central
//! `AudioCacheTrackMetadata` (implémentation de `pmometadata::TrackMetadata`).
//! Elle ne touche plus directement la base de données ni ne s'appuie sur des
//! structures parallèles : toutes les lectures passent par `TrackMetadata`.

use crate::{AudioCacheTrackMetadata, AudioConfig};
use pmometadata::{MetadataError, TrackMetadata};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Fournit un accès direct à une implémentation `TrackMetadata` basée sur le cache.
///
/// Utilise `AudioCacheTrackMetadata` comme backend, ce qui garantit que toutes
/// les lectures/écritures passent par la DB `pmocache` sans recharger le FLAC.
pub trait AudioTrackMetadataExt {
    fn track_metadata(&self, pk: impl Into<String>) -> Arc<RwLock<dyn TrackMetadata>>;
}

impl AudioTrackMetadataExt for Arc<pmocache::Cache<AudioConfig>> {
    fn track_metadata(&self, pk: impl Into<String>) -> Arc<RwLock<dyn TrackMetadata>> {
        let metadata = AudioCacheTrackMetadata::new(self.clone(), pk);
        Arc::new(RwLock::new(metadata))
    }
}

/// Accès « léger » aux principales métadonnées en s'appuyant sur `TrackMetadata`.
///
/// Cette version remplace l'ancienne macro `define_metadata_properties!` qui
/// accédait directement aux clés de la DB. Les méthodes restent asynchrones et
/// retournent `Option` ; en cas d'erreur backend, elles lèvent `anyhow::Error`.
/// Utile dans les handlers HTTP ou les services qui n'ont besoin que de quelques
/// champs sans manipuler explicitement un `TrackMetadata`.
pub trait AudioMetadataExt {
    async fn get_title(&self, pk: &str) -> anyhow::Result<Option<String>>;
    async fn get_artist(&self, pk: &str) -> anyhow::Result<Option<String>>;
    async fn get_album(&self, pk: &str) -> anyhow::Result<Option<String>>;
    async fn get_duration_secs(&self, pk: &str) -> anyhow::Result<Option<i64>>;
}

impl AudioMetadataExt for Arc<pmocache::Cache<AudioConfig>> {
    async fn get_title(&self, pk: &str) -> anyhow::Result<Option<String>> {
        let meta = self.track_metadata(pk);
        let res = {
            let guard = meta.read().await;
            guard.get_title().await
        };
        res.map_err(|e| map_err("title", pk, e))
    }

    async fn get_artist(&self, pk: &str) -> anyhow::Result<Option<String>> {
        let meta = self.track_metadata(pk);
        let res = {
            let guard = meta.read().await;
            guard.get_artist().await
        };
        res.map_err(|e| map_err("artist", pk, e))
    }

    async fn get_album(&self, pk: &str) -> anyhow::Result<Option<String>> {
        let meta = self.track_metadata(pk);
        let res = {
            let guard = meta.read().await;
            guard.get_album().await
        };
        res.map_err(|e| map_err("album", pk, e))
    }

    async fn get_duration_secs(&self, pk: &str) -> anyhow::Result<Option<i64>> {
        let meta = self.track_metadata(pk);
        let res = {
            let guard = meta.read().await;
            guard.get_duration().await
        };
        let duration = res.map_err(|e| map_err("duration", pk, e))?;
        Ok(duration.map(|d| d.as_secs() as i64))
    }
}

fn map_err(field: &str, pk: &str, err: MetadataError) -> anyhow::Error {
    match err {
        MetadataError::NotImplemented | MetadataError::ReadOnly => {
            anyhow::anyhow!("metadata {field} for {pk} is not implemented")
        }
        MetadataError::Backend(msg) => {
            anyhow::anyhow!("backend error on {field} for {pk}: {msg}")
        }
    }
}

/// Extension pour convertir TrackMetadata en Resource DIDL-Lite (UPnP)
#[async_trait::async_trait]
pub trait TrackMetadataDidlExt {
    /// Convertit les métadonnées en Resource DIDL-Lite
    ///
    /// # Arguments
    ///
    /// * `url` - URL de la ressource audio
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// use pmoaudiocache::metadata_ext::{AudioTrackMetadataExt, TrackMetadataDidlExt};
    ///
    /// let track_meta = cache.track_metadata(&pk);
    /// let resource = track_meta.read().await.to_didl_resource("http://example.com/track.flac".to_string()).await;
    /// ```
    async fn to_didl_resource(&self, url: String) -> pmodidl::Resource;
}

#[async_trait::async_trait]
impl TrackMetadataDidlExt for dyn TrackMetadata {
    async fn to_didl_resource(&self, url: String) -> pmodidl::Resource {
        // Récupérer la durée et la formater pour DIDL-Lite (H:MM:SS)
        let duration = self.get_duration().await.ok().flatten().map(|d| {
            let secs = d.as_secs();
            let hours = secs / 3600;
            let minutes = (secs % 3600) / 60;
            let seconds = secs % 60;
            format!("{}:{:02}:{:02}", hours, minutes, seconds)
        });

        pmodidl::Resource {
            // Aligne sur Sink du renderer (audio/flac) avec PN explicite.
            protocol_info: "http-get:*:audio/flac:DLNA.ORG_PN=FLAC".to_string(),
            bits_per_sample: self
                .get_bits_per_sample()
                .await
                .ok()
                .flatten()
                .map(|b| b.to_string()),
            sample_frequency: self
                .get_sample_rate()
                .await
                .ok()
                .flatten()
                .map(|sr| sr.to_string()),
            nr_audio_channels: self
                .get_channels()
                .await
                .ok()
                .flatten()
                .map(|ch| ch.to_string()),
            duration,
            url,
        }
    }
}
