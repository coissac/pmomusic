use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pmometadata::{MetadataError, MetadataResult, TrackMetadata};
use serde_json::{Number, Value};

fn map_db_err(err: rusqlite::Error) -> MetadataError {
    MetadataError::Backend(err.to_string())
}

/// Implémentation `TrackMetadata` adossée au cache audio.
///
/// Cette couche lit/écrit directement dans la table `metadata` de `pmocache`
/// pour un `pk` donné. Elle se comporte comme une façade `TrackMetadata`
/// classique mais repose sur la DB du cache plutôt que sur un fichier taggé,
/// ce qui permet :
/// - d'exposer les métadonnées immédiatement après ingestion/transform ;
/// - de servir des lecteurs UPnP/DLNA sans relire le FLAC sur disque ;
/// - de persister les mises à jour d'un client (ex: renommer un titre).
pub struct AudioCacheTrackMetadata {
    cache: Arc<crate::Cache>,
    pk: String,
}

impl AudioCacheTrackMetadata {
    /// Construit un adaptateur `TrackMetadata` pour un `pk` du cache audio.
    ///
    /// Le type implémente ensuite toutes les méthodes du trait `pmometadata::TrackMetadata`
    /// en stockant les données dans la base SQLite de `pmocache`.
    pub fn new(cache: Arc<crate::Cache>, pk: impl Into<String>) -> Self {
        Self {
            cache,
            pk: pk.into(),
        }
    }

    fn read_raw(&self, key: &str) -> Result<Option<Value>, MetadataError> {
        self.cache
            .db
            .get_a_metadata(&self.pk, key)
            .map_err(map_db_err)
    }

    fn write_raw(&self, key: &str, value: Value) -> Result<(), MetadataError> {
        self.cache
            .db
            .set_a_metadata(&self.pk, key, value)
            .map_err(map_db_err)
    }

    fn read_string(&self, key: &str) -> Result<Option<String>, MetadataError> {
        match self.read_raw(key)? {
            Some(Value::String(s)) => Ok(Some(s)),
            Some(Value::Null) | None => Ok(None),
            Some(other) => Err(MetadataError::Backend(format!(
                "metadata {key} for {} is not a string ({other})",
                self.pk
            ))),
        }
    }

    fn write_string(&self, key: &str, value: Option<String>) -> Result<(), MetadataError> {
        let json = value.map(Value::String).unwrap_or(Value::Null);
        self.write_raw(key, json)
    }

    fn read_number(&self, key: &str) -> Result<Option<Number>, MetadataError> {
        match self.read_raw(key)? {
            Some(Value::Number(n)) => Ok(Some(n)),
            Some(Value::Null) | None => Ok(None),
            Some(other) => Err(MetadataError::Backend(format!(
                "metadata {key} for {} is not a number ({other})",
                self.pk
            ))),
        }
    }

    fn write_number(&self, key: &str, value: Option<i64>) -> Result<(), MetadataError> {
        let json = match value {
            Some(n) => Value::Number(Number::from(n)),
            None => Value::Null,
        };
        self.write_raw(key, json)
    }

    fn write_u64(&self, key: &str, value: Option<u64>) -> Result<(), MetadataError> {
        let json = match value {
            Some(n) => Value::Number(Number::from(n)),
            None => Value::Null,
        };
        self.write_raw(key, json)
    }

    fn write_f64(&self, key: &str, value: Option<f64>) -> Result<(), MetadataError> {
        let json = match value {
            Some(v) => Number::from_f64(v)
                .map(Value::Number)
                .ok_or_else(|| MetadataError::Backend(format!("invalid float for {key}")))?,
            None => Value::Null,
        };
        self.write_raw(key, json)
    }

    fn read_duration(&self) -> Result<Option<Duration>, MetadataError> {
        match self.read_number("duration_secs")? {
            Some(n) => match n.as_u64() {
                Some(secs) => Ok(Some(Duration::from_secs(secs))),
                None => Err(MetadataError::Backend(format!(
                    "duration_secs for {} out of range",
                    self.pk
                ))),
            },
            None => Ok(None),
        }
    }

    fn write_duration(&self, value: Option<Duration>) -> Result<(), MetadataError> {
        self.write_u64("duration_secs", value.map(|d| d.as_secs()))
    }

    fn read_timestamp(&self) -> Result<Option<SystemTime>, MetadataError> {
        match self.read_number("updated_at")? {
            Some(n) => match n.as_u64() {
                Some(secs) => Ok(Some(UNIX_EPOCH + Duration::from_secs(secs))),
                None => Err(MetadataError::Backend(format!(
                    "updated_at for {} out of range",
                    self.pk
                ))),
            },
            None => Ok(None),
        }
    }

    fn write_timestamp(&self, when: SystemTime) -> Result<(), MetadataError> {
        let secs = when
            .duration_since(UNIX_EPOCH)
            .map_err(|e| MetadataError::Backend(e.to_string()))?
            .as_secs();
        self.write_u64("updated_at", Some(secs))
    }
}

#[async_trait::async_trait]
impl TrackMetadata for AudioCacheTrackMetadata {
    async fn get_title(&self) -> MetadataResult<String> {
        Ok(self.read_string("title")?)
    }

    async fn set_title(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.write_string("title", value)?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_artist(&self) -> MetadataResult<String> {
        Ok(self.read_string("artist")?)
    }

    async fn set_artist(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.write_string("artist", value)?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_album(&self) -> MetadataResult<String> {
        Ok(self.read_string("album")?)
    }

    async fn set_album(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.write_string("album", value)?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_year(&self) -> MetadataResult<u32> {
        Ok(match self.read_number("year")? {
            Some(n) => n
                .as_i64()
                .and_then(|v| u32::try_from(v).ok())
                .map(Some)
                .unwrap_or(None),
            None => None,
        })
    }

    async fn set_year(&mut self, value: Option<u32>) -> MetadataResult<()> {
        self.write_number("year", value.map(|v| v as i64))?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_genre(&self) -> MetadataResult<String> {
        Ok(self.read_string("genre")?)
    }

    async fn set_genre(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.write_string("genre", value)?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_track_number(&self) -> MetadataResult<u32> {
        Ok(match self.read_number("track_number")? {
            Some(n) => n.as_i64().and_then(|v| u32::try_from(v).ok()),
            None => None,
        })
    }

    async fn set_track_number(&mut self, value: Option<u32>) -> MetadataResult<()> {
        self.write_number("track_number", value.map(|v| v as i64))?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_track_total(&self) -> MetadataResult<u32> {
        Ok(match self.read_number("track_total")? {
            Some(n) => n.as_i64().and_then(|v| u32::try_from(v).ok()),
            None => None,
        })
    }

    async fn set_track_total(&mut self, value: Option<u32>) -> MetadataResult<()> {
        self.write_number("track_total", value.map(|v| v as i64))?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_disc_number(&self) -> MetadataResult<u32> {
        Ok(match self.read_number("disc_number")? {
            Some(n) => n.as_i64().and_then(|v| u32::try_from(v).ok()),
            None => None,
        })
    }

    async fn set_disc_number(&mut self, value: Option<u32>) -> MetadataResult<()> {
        self.write_number("disc_number", value.map(|v| v as i64))?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_disc_total(&self) -> MetadataResult<u32> {
        Ok(match self.read_number("disc_total")? {
            Some(n) => n.as_i64().and_then(|v| u32::try_from(v).ok()),
            None => None,
        })
    }

    async fn set_disc_total(&mut self, value: Option<u32>) -> MetadataResult<()> {
        self.write_number("disc_total", value.map(|v| v as i64))?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_duration(&self) -> MetadataResult<Duration> {
        Ok(self.read_duration()?)
    }

    async fn set_duration(&mut self, value: Option<Duration>) -> MetadataResult<()> {
        self.write_duration(value)?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_sample_rate(&self) -> MetadataResult<u32> {
        Ok(match self.read_number("sample_rate")? {
            Some(n) => n.as_i64().and_then(|v| u32::try_from(v).ok()),
            None => None,
        })
    }

    async fn set_sample_rate(&mut self, value: Option<u32>) -> MetadataResult<()> {
        self.write_number("sample_rate", value.map(|v| v as i64))?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_total_samples(&self) -> MetadataResult<u64> {
        Ok(match self.read_number("total_samples")? {
            Some(n) => n.as_i64().and_then(|v| u64::try_from(v).ok()),
            None => None,
        })
    }

    async fn set_total_samples(&mut self, value: Option<u64>) -> MetadataResult<()> {
        self.write_u64("total_samples", value)?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_bits_per_sample(&self) -> MetadataResult<u8> {
        Ok(match self.read_number("bits_per_sample")? {
            Some(n) => n.as_i64().and_then(|v| u8::try_from(v).ok()),
            None => None,
        })
    }

    async fn set_bits_per_sample(&mut self, value: Option<u8>) -> MetadataResult<()> {
        self.write_number("bits_per_sample", value.map(|v| v as i64))?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_channels(&self) -> MetadataResult<u8> {
        Ok(match self.read_number("channels")? {
            Some(n) => n.as_i64().and_then(|v| u8::try_from(v).ok()),
            None => None,
        })
    }

    async fn set_channels(&mut self, value: Option<u8>) -> MetadataResult<()> {
        self.write_number("channels", value.map(|v| v as i64))?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_bitrate(&self) -> MetadataResult<u32> {
        Ok(match self.read_number("bitrate")? {
            Some(n) => n.as_i64().and_then(|v| u32::try_from(v).ok()),
            None => None,
        })
    }

    async fn set_bitrate(&mut self, value: Option<u32>) -> MetadataResult<()> {
        self.write_number("bitrate", value.map(|v| v as i64))?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_track_id(&self) -> MetadataResult<String> {
        Ok(self.read_string("track_id")?)
    }

    async fn set_track_id(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.write_string("track_id", value)?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_channel_id(&self) -> MetadataResult<String> {
        Ok(self.read_string("channel_id")?)
    }

    async fn set_channel_id(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.write_string("channel_id", value)?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_event(&self) -> MetadataResult<String> {
        Ok(self.read_string("event")?)
    }

    async fn set_event(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.write_string("event", value)?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_rating(&self) -> MetadataResult<f32> {
        Ok(match self.read_number("rating")? {
            Some(n) => n.as_f64().map(|v| v as f32),
            None => None,
        })
    }

    async fn set_rating(&mut self, value: Option<f32>) -> MetadataResult<()> {
        self.write_f64("rating", value.map(|v| v as f64))?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_cover_url(&self) -> MetadataResult<String> {
        Ok(self.read_string("cover_url")?)
    }

    async fn set_cover_url(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.write_string("cover_url", value)?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_cover_pk(&self) -> MetadataResult<String> {
        Ok(self.read_string("cover_pk")?)
    }

    async fn set_cover_pk(&mut self, value: Option<String>) -> MetadataResult<()> {
        self.write_string("cover_pk", value)?;
        let _ = self.touch().await?;
        Ok(Some(()))
    }

    async fn get_updated_at(&self) -> MetadataResult<SystemTime> {
        Ok(self.read_timestamp()?)
    }

    async fn touch(&mut self) -> MetadataResult<()> {
        self.write_timestamp(Instant::now())?;
        Ok(Some(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::new_cache;
    use crate::metadata_ext::AudioTrackMetadataExt;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[tokio::test]
    async fn roundtrip_metadata() {
        let dir = tempdir().unwrap();
        let cache = Arc::new(new_cache(dir.path().to_str().unwrap(), 4).unwrap());
        let pk = "track-test";
        cache.db.add(pk, None, None).unwrap();

        let track = cache.track_metadata(pk);

        {
            let mut meta = track.write().await;

            meta.set_title(Some("Title".into())).await.unwrap();
            meta.set_artist(Some("Artist".into())).await.unwrap();
            meta.set_album(Some("Album".into())).await.unwrap();
            meta.set_year(Some(2024)).await.unwrap();
            meta.set_duration(Some(Duration::from_secs(90)))
                .await
                .unwrap();
            meta.set_sample_rate(Some(44100)).await.unwrap();
            meta.set_total_samples(Some(9_999_999)).await.unwrap();
            meta.set_bits_per_sample(Some(16)).await.unwrap();
            meta.set_track_id(Some("trk".into())).await.unwrap();
            meta.set_channel_id(Some("chn".into())).await.unwrap();
            meta.set_event(Some("event".into())).await.unwrap();
            meta.set_rating(Some(4.5)).await.unwrap();
            meta.set_cover_url(Some("http://cover".into()))
                .await
                .unwrap();
            meta.set_cover_pk(Some("cover123".into())).await.unwrap();
        }
        {
            let meta = track.read().await;

            assert_eq!(meta.get_title().await.unwrap(), Some("Title".into()));
            assert_eq!(meta.get_artist().await.unwrap(), Some("Artist".into()));
            assert_eq!(meta.get_album().await.unwrap(), Some("Album".into()));
            assert_eq!(meta.get_year().await.unwrap(), Some(2024));
            assert_eq!(
                meta.get_duration().await.unwrap(),
                Some(Duration::from_secs(90))
            );
            assert_eq!(meta.get_sample_rate().await.unwrap(), Some(44100));
            assert_eq!(meta.get_total_samples().await.unwrap(), Some(9_999_999));
            assert_eq!(meta.get_bits_per_sample().await.unwrap(), Some(16));
            assert_eq!(meta.get_track_id().await.unwrap(), Some("trk".into()));
            assert_eq!(meta.get_channel_id().await.unwrap(), Some("chn".into()));
            assert_eq!(meta.get_event().await.unwrap(), Some("event".into()));
            assert_eq!(meta.get_rating().await.unwrap(), Some(4.5));
            assert_eq!(
                meta.get_cover_url().await.unwrap(),
                Some("http://cover".into())
            );
            assert_eq!(meta.get_cover_pk().await.unwrap(), Some("cover123".into()));
            assert!(meta.get_updated_at().await.unwrap().is_some());
        }
    }
}
