use std::sync::Arc;
use tokio::sync::RwLock;

use pmometadata::TrackMetadata;

use crate::{gain_db_from_linear, AudioChunk, AudioChunkData, BitDepth, SyncMarker};

pub enum _AudioSegment {
    Chunk(Arc<AudioChunk>),
    Sync(Arc<SyncMarker>),
}

pub struct AudioSegment {
    pub order: u64,
    pub timestamp_sec: f64,
    pub segment: _AudioSegment,
}

impl AudioSegment {
    /// Crée un nouveau segment audio depuis des frames i32
    pub fn new_chunk(
        order: u64,
        timestamp_sec: f64,
        stereo: Vec<[i32; 2]>,
        sample_rate: u32,
        _bit_depth: BitDepth, // Conservé pour compatibilité API
    ) -> Arc<Self> {
        let chunk_data = AudioChunkData::new(stereo, sample_rate, 0.0);
        let chunk = AudioChunk::I32(chunk_data);
        Arc::new(Self {
            order,
            timestamp_sec,
            segment: _AudioSegment::Chunk(Arc::new(chunk)),
        })
    }

    /// Crée un nouveau segment audio avec gain (dB)
    pub fn new_chunk_with_gain_db(
        order: u64,
        timestamp_sec: f64,
        stereo: Vec<[i32; 2]>,
        sample_rate: u32,
        _bit_depth: BitDepth, // Conservé pour compatibilité API
        gain_db: f64,
    ) -> Arc<Self> {
        let chunk_data = AudioChunkData::new(stereo, sample_rate, gain_db);
        let chunk = AudioChunk::I32(chunk_data);
        Arc::new(Self {
            order,
            timestamp_sec,
            segment: _AudioSegment::Chunk(Arc::new(chunk)),
        })
    }

    /// Crée un nouveau segment audio avec gain linéaire
    pub fn new_chunk_with_gain_linear(
        order: u64,
        timestamp_sec: f64,
        stereo: Vec<[i32; 2]>,
        sample_rate: u32,
        _bit_depth: BitDepth, // Conservé pour compatibilité API
        gain_linear: f64,
    ) -> Arc<Self> {
        let chunk_data = AudioChunkData::new(stereo, sample_rate, gain_db_from_linear(gain_linear));
        let chunk = AudioChunk::I32(chunk_data);
        Arc::new(Self {
            order,
            timestamp_sec,
            segment: _AudioSegment::Chunk(Arc::new(chunk)),
        })
    }

    /// Crée un segment audio depuis deux canaux i32 séparés (L/R)
    pub fn new_chunk_from_channels_i32(
        order: u64,
        timestamp_sec: f64,
        left: Vec<i32>,
        right: Vec<i32>,
        sample_rate: u32,
        _bit_depth: BitDepth, // Conservé pour compatibilité API
    ) -> Arc<Self> {
        let chunk_data = AudioChunkData::<i32>::from_channels(left, right, sample_rate);
        let chunk = AudioChunk::I32(chunk_data);
        Arc::new(Self {
            order,
            timestamp_sec,
            segment: _AudioSegment::Chunk(Arc::new(chunk)),
        })
    }

    /// Crée un segment audio depuis deux canaux f32 normalisés (L/R)
    ///
    /// Convertit f32 normalisé [-1.0, 1.0] → i32 selon le bit_depth spécifié
    pub fn new_chunk_from_channels_f32(
        order: u64,
        timestamp_sec: f64,
        left: Vec<f32>,
        right: Vec<f32>,
        sample_rate: u32,
        bit_depth: BitDepth,
    ) -> Arc<Self> {
        assert_eq!(
            left.len(),
            right.len(),
            "channels must have identical length"
        );

        // Convertir f32 → i32 selon le bit_depth
        let max_value = bit_depth.max_value();
        let stereo: Vec<[i32; 2]> = left
            .into_iter()
            .zip(right.into_iter())
            .map(|(l, r)| {
                let l_scaled = (l * max_value).clamp(-max_value, max_value - 1.0).round() as i32;
                let r_scaled = (r * max_value).clamp(-max_value, max_value - 1.0).round() as i32;
                [l_scaled, r_scaled]
            })
            .collect();

        let chunk_data = AudioChunkData::new(stereo, sample_rate, 0.0);
        let chunk = AudioChunk::I32(chunk_data);
        Arc::new(Self {
            order,
            timestamp_sec,
            segment: _AudioSegment::Chunk(Arc::new(chunk)),
        })
    }

    /// Crée un segment audio depuis des frames f32 normalisées
    ///
    /// Convertit f32 normalisé [-1.0, 1.0] → i32 selon le bit_depth spécifié
    pub fn new_chunk_from_pairs_f32(
        order: u64,
        timestamp_sec: f64,
        pairs: Vec<[f32; 2]>,
        sample_rate: u32,
        bit_depth: BitDepth,
    ) -> Arc<Self> {
        // Convertir f32 → i32 selon le bit_depth
        let max_value = bit_depth.max_value();
        let stereo: Vec<[i32; 2]> = pairs
            .into_iter()
            .map(|[l, r]| {
                let l_scaled = (l * max_value).clamp(-max_value, max_value - 1.0).round() as i32;
                let r_scaled = (r * max_value).clamp(-max_value, max_value - 1.0).round() as i32;
                [l_scaled, r_scaled]
            })
            .collect();

        let chunk_data = AudioChunkData::new(stereo, sample_rate, 0.0);
        let chunk = AudioChunk::I32(chunk_data);
        Arc::new(Self {
            order,
            timestamp_sec,
            segment: _AudioSegment::Chunk(Arc::new(chunk)),
        })
    }

    pub fn new_track_boundary(
        order: u64,
        timestamp_sec: f64,
        metadata: Arc<RwLock<dyn TrackMetadata>>,
    ) -> Arc<Self> {
        let marker = Arc::new(SyncMarker::TrackBoundary {
            metadata: Arc::clone(&metadata),
        });
        Arc::new(Self {
            order,
            timestamp_sec,
            segment: _AudioSegment::Sync(marker),
        })
    }

    pub fn new_stream_metadata(
        order: u64,
        timestamp_sec: f64,
        key: String,
        value: String,
    ) -> Arc<Self> {
        let marker = Arc::new(SyncMarker::StreamMetadata { key, value });

        Arc::new(Self {
            order,
            timestamp_sec,
            segment: _AudioSegment::Sync(marker),
        })
    }

    pub fn new_top_zero_sync() -> Arc<Self> {
        let marker = Arc::new(SyncMarker::TopZeroSync);

        Arc::new(Self {
            order: 0,
            timestamp_sec: 0.0,
            segment: _AudioSegment::Sync(marker),
        })
    }

    pub fn new_heartbeat(order: u64, timestamp_sec: f64) -> Arc<Self> {
        let marker = Arc::new(SyncMarker::Heartbeat);

        Arc::new(Self {
            order,
            timestamp_sec,
            segment: _AudioSegment::Sync(marker),
        })
    }

    pub fn new_end_of_stream(order: u64, timestamp_sec: f64) -> Arc<Self> {
        let marker = Arc::new(SyncMarker::EndOfStream);

        Arc::new(Self {
            order: order,
            timestamp_sec: timestamp_sec,
            segment: _AudioSegment::Sync(marker),
        })
    }

    pub fn new_error(order: u64, timestamp_sec: f64, error: String) -> Arc<Self> {
        let marker = Arc::new(SyncMarker::Error(error));

        Arc::new(Self {
            order: order,
            timestamp_sec: timestamp_sec,
            segment: _AudioSegment::Sync(marker),
        })
    }

    pub fn is_audio_chunk(&self) -> bool {
        matches!(self.segment, _AudioSegment::Chunk(_))
    }

    pub fn is_track_boundary(&self) -> bool {
        matches!(
            self.segment,
            _AudioSegment::Sync(ref marker)
                if matches!(**marker,
                    SyncMarker::TrackBoundary { .. }
                )
        )
    }

    pub fn is_stream_metadata(&self) -> bool {
        matches!(
            self.segment,
            _AudioSegment::Sync(ref marker)
                if matches!(**marker,
                    SyncMarker::StreamMetadata { .. }
                )
        )
    }
    pub fn is_heartbeat(&self) -> bool {
        matches!(
            self.segment,
            _AudioSegment::Sync(ref marker)
            if matches!(**marker, SyncMarker::Heartbeat)
        )
    }

    pub fn is_top_zero_sync(&self) -> bool {
        matches!(
            self.segment,
            _AudioSegment::Sync(ref marker)
            if matches!(**marker, SyncMarker::TopZeroSync)
        )
    }

    pub fn is_end_of_stream(&self) -> bool {
        matches!(
            self.segment,
            _AudioSegment::Sync(ref marker)
            if matches!(**marker, SyncMarker::EndOfStream)
        )
    }

    pub fn is_error(&self) -> bool {
        matches!(
            self.segment,
            _AudioSegment::Sync(ref marker)
            if matches!(**marker, SyncMarker::Error(_))
        )
    }

    // ============ Accesseurs typés pour AudioChunk ============

    /// Récupère le AudioChunk si ce segment est un chunk audio
    pub fn as_chunk(&self) -> Option<&Arc<AudioChunk>> {
        match &self.segment {
            _AudioSegment::Chunk(chunk) => Some(chunk),
            _ => None,
        }
    }

    /// Récupère le SyncMarker si ce segment est un marqueur de sync
    pub fn as_sync_marker(&self) -> Option<&Arc<SyncMarker>> {
        match &self.segment {
            _AudioSegment::Sync(marker) => Some(marker),
            _ => None,
        }
    }

    /// Récupère les métadatas du track si c'est un TrackBoundary
    pub fn as_track_metadata(&self) -> Option<&Arc<RwLock<dyn TrackMetadata>>> {
        match &self.segment {
            _AudioSegment::Sync(marker) => match &**marker {
                SyncMarker::TrackBoundary { metadata } => Some(metadata),
                _ => None,
            },
            _ => None,
        }
    }

    /// Récupère le message d'erreur si c'est un marqueur Error
    pub fn as_error(&self) -> Option<&str> {
        match &self.segment {
            _AudioSegment::Sync(marker) => match &**marker {
                SyncMarker::Error(msg) => Some(msg.as_str()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Convertit l'AudioChunk vers F32 si c'est un chunk audio
    pub fn to_f32_chunk(&self) -> Option<AudioChunk> {
        self.as_chunk().map(|chunk| chunk.to_f32())
    }

    /// Convertit l'AudioChunk vers I32 si c'est un chunk audio
    pub fn to_i32_chunk(&self) -> Option<AudioChunk> {
        self.as_chunk().map(|chunk| chunk.to_i32())
    }

    /// Récupère le sample rate du chunk audio
    pub fn sample_rate(&self) -> Option<u32> {
        self.as_chunk().map(|chunk| chunk.sample_rate())
    }

    /// Récupère le nombre de frames du chunk audio
    pub fn frame_count(&self) -> Option<usize> {
        self.as_chunk().map(|chunk| chunk.len())
    }

    /// Récupère le gain en dB du chunk audio
    pub fn gain_db(&self) -> Option<f64> {
        self.as_chunk().map(|chunk| chunk.gain_db())
    }

    /// Récupère le type du chunk audio (nom du type: "i32", "f32", etc.)
    pub fn chunk_type_name(&self) -> Option<&'static str> {
        self.as_chunk().map(|chunk| chunk.type_name())
    }

    /// Crée un nouveau segment avec le gain modifié (si c'est un chunk audio)
    pub fn with_gain_db(&self, gain_db: f64) -> Option<Arc<Self>> {
        self.as_chunk().map(|chunk| {
            let new_chunk = chunk.set_gain_db(gain_db);
            Arc::new(Self {
                order: self.order,
                timestamp_sec: self.timestamp_sec,
                segment: _AudioSegment::Chunk(Arc::new(new_chunk)),
            })
        })
    }

    /// Crée un nouveau segment avec le gain ajusté (relatif, si c'est un chunk audio)
    pub fn adjust_gain_db(&self, delta_db: f64) -> Option<Arc<Self>> {
        self.as_chunk().map(|chunk| {
            let new_gain = chunk.gain_db() + delta_db;
            let new_chunk = chunk.set_gain_db(new_gain);
            Arc::new(Self {
                order: self.order,
                timestamp_sec: self.timestamp_sec,
                segment: _AudioSegment::Chunk(Arc::new(new_chunk)),
            })
        })
    }
}

impl TryInto<Arc<AudioChunk>> for AudioSegment {
    type Error = ();

    fn try_into(self) -> Result<Arc<AudioChunk>, Self::Error> {
        match self.segment {
            _AudioSegment::Chunk(chunk) => Ok(chunk),
            _ => Err(()),
        }
    }
}

impl TryInto<Arc<SyncMarker>> for AudioSegment {
    type Error = ();

    fn try_into(self) -> Result<Arc<SyncMarker>, Self::Error> {
        match self.segment {
            _AudioSegment::Sync(marker) => Ok(marker),
            _ => Err(()),
        }
    }
}

impl<'a> TryInto<&'a Arc<AudioChunk>> for &'a AudioSegment {
    type Error = ();

    fn try_into(self) -> Result<&'a Arc<AudioChunk>, Self::Error> {
        match &self.segment {
            _AudioSegment::Chunk(ref chunk) => Ok(chunk),
            _ => Err(()),
        }
    }
}

impl<'a> TryInto<&'a Arc<SyncMarker>> for &'a AudioSegment {
    type Error = ();

    fn try_into(self) -> Result<&'a Arc<SyncMarker>, Self::Error> {
        match &self.segment {
            _AudioSegment::Sync(ref marker) => Ok(marker),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_segment_accessors() {
        // Test avec un chunk audio
        let segment = AudioSegment::new_chunk(
            42,
            1.5,
            vec![[100i32, 200i32], [300i32, 400i32]],
            48000,
            BitDepth::B32,
        );

        assert!(segment.is_audio_chunk());
        assert!(!segment.is_heartbeat());
        assert!(segment.as_chunk().is_some());
        assert!(segment.as_sync_marker().is_none());
        assert_eq!(segment.sample_rate(), Some(48000));
        assert_eq!(segment.frame_count(), Some(2));
        assert_eq!(segment.gain_db(), Some(0.0));
        assert_eq!(segment.chunk_type_name(), Some("i32"));

        // Test avec un marqueur sync
        let sync_segment = AudioSegment::new_heartbeat(10, 2.0);
        assert!(!sync_segment.is_audio_chunk());
        assert!(sync_segment.is_heartbeat());
        assert!(sync_segment.as_chunk().is_none());
        assert!(sync_segment.as_sync_marker().is_some());
        assert_eq!(sync_segment.sample_rate(), None);
    }

    #[test]
    fn test_audio_segment_gain_manipulation() {
        let segment = AudioSegment::new_chunk(0, 0.0, vec![[100i32, 200i32]], 44100, BitDepth::B32);

        // Test with_gain_db
        let segment_6db = segment.with_gain_db(6.0).unwrap();
        assert_eq!(segment_6db.gain_db(), Some(6.0));
        assert_eq!(segment_6db.order, 0);
        assert_eq!(segment_6db.timestamp_sec, 0.0);

        // Test adjust_gain_db
        let segment_plus_3db = segment_6db.adjust_gain_db(3.0).unwrap();
        assert_eq!(segment_plus_3db.gain_db(), Some(9.0));

        // Test sur un sync marker (devrait retourner None)
        let sync = AudioSegment::new_heartbeat(1, 1.0);
        assert!(sync.with_gain_db(6.0).is_none());
        assert!(sync.adjust_gain_db(3.0).is_none());
    }

    #[test]
    fn test_audio_segment_conversions() {
        let segment =
            AudioSegment::new_chunk(0, 0.0, vec![[1000000i32, 2000000i32]], 44100, BitDepth::B32);

        // Test to_f32_chunk
        let f32_chunk = segment.to_f32_chunk();
        assert!(f32_chunk.is_some());
        assert_eq!(f32_chunk.unwrap().type_name(), "f32");

        // Test to_i32_chunk
        let i32_chunk = segment.to_i32_chunk();
        assert!(i32_chunk.is_some());
        assert_eq!(i32_chunk.unwrap().type_name(), "i32");

        // Test sur un sync marker
        let sync = AudioSegment::new_heartbeat(1, 1.0);
        assert!(sync.to_f32_chunk().is_none());
        assert!(sync.to_i32_chunk().is_none());
    }

    #[test]
    fn test_audio_segment_error_marker() {
        let error_msg = "Test error message";
        let segment = AudioSegment::new_error(5, 2.5, error_msg.to_string());

        assert!(segment.is_error());
        assert_eq!(segment.as_error(), Some(error_msg));

        // Autre type de segment ne devrait pas être une erreur
        let sync = AudioSegment::new_heartbeat(1, 1.0);
        assert!(!sync.is_error());
        assert_eq!(sync.as_error(), None);
    }
}
