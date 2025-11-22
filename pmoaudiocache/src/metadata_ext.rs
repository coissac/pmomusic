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
