/// Macros pour simplifier la manipulation des AudioChunk et AudioSegment

/// Extrait les données typées d'un AudioChunk
///
/// # Exemples
/// ```
/// use pmoaudio::{AudioChunk, AudioChunkData, extract_chunk_data};
///
/// fn process_i32(chunk: &AudioChunk) {
///     if let Some(data) = extract_chunk_data!(chunk, I32) {
///         println!("I32 chunk with {} frames", data.len());
///     }
/// }
/// ```
#[macro_export]
macro_rules! extract_chunk_data {
    ($chunk:expr, I16) => {
        match $chunk {
            $crate::AudioChunk::I16(data) => Some(data),
            _ => None,
        }
    };
    ($chunk:expr, I24) => {
        match $chunk {
            $crate::AudioChunk::I24(data) => Some(data),
            _ => None,
        }
    };
    ($chunk:expr, I32) => {
        match $chunk {
            $crate::AudioChunk::I32(data) => Some(data),
            _ => None,
        }
    };
    ($chunk:expr, F32) => {
        match $chunk {
            $crate::AudioChunk::F32(data) => Some(data),
            _ => None,
        }
    };
    ($chunk:expr, F64) => {
        match $chunk {
            $crate::AudioChunk::F64(data) => Some(data),
            _ => None,
        }
    };
}

/// Match sur le type d'un AudioChunk avec exécution de code pour chaque cas
///
/// # Exemples
/// ```
/// use pmoaudio::{AudioChunk, match_chunk};
///
/// fn print_chunk_info(chunk: &AudioChunk) {
///     match_chunk!(chunk, data => {
///         println!("Chunk type: {}, frames: {}", chunk.type_name(), data.len());
///     });
/// }
/// ```
#[macro_export]
macro_rules! match_chunk {
    ($chunk:expr, $data:ident => $body:expr) => {
        match $chunk {
            $crate::AudioChunk::I16($data) => $body,
            $crate::AudioChunk::I24($data) => $body,
            $crate::AudioChunk::I32($data) => $body,
            $crate::AudioChunk::F32($data) => $body,
            $crate::AudioChunk::F64($data) => $body,
        }
    };
}

/// Map sur un AudioChunk - transforme les données et retourne un nouveau AudioChunk du même type
///
/// # Exemples
/// ```
/// use pmoaudio::{AudioChunk, map_chunk};
///
/// fn add_gain_db(chunk: &AudioChunk, gain_db: f64) -> AudioChunk {
///     map_chunk!(chunk, data => {
///         data.set_gain_db(data.gain_db() + gain_db)
///     })
/// }
/// ```
#[macro_export]
macro_rules! map_chunk {
    ($chunk:expr, $data:ident => $transform:expr) => {
        match $chunk {
            $crate::AudioChunk::I16($data) => $crate::AudioChunk::I16($transform),
            $crate::AudioChunk::I24($data) => $crate::AudioChunk::I24($transform),
            $crate::AudioChunk::I32($data) => $crate::AudioChunk::I32($transform),
            $crate::AudioChunk::F32($data) => $crate::AudioChunk::F32($transform),
            $crate::AudioChunk::F64($data) => $crate::AudioChunk::F64($transform),
        }
    };
}

/// Prédicat sur le type d'un AudioChunk
///
/// # Exemples
/// ```
/// use pmoaudio::{AudioChunk, is_chunk_type};
///
/// fn process_only_i32(chunk: &AudioChunk) {
///     if is_chunk_type!(chunk, I32) {
///         println!("Processing I32 chunk");
///     }
/// }
/// ```
#[macro_export]
macro_rules! is_chunk_type {
    ($chunk:expr, I16) => {
        matches!($chunk, $crate::AudioChunk::I16(_))
    };
    ($chunk:expr, I24) => {
        matches!($chunk, $crate::AudioChunk::I24(_))
    };
    ($chunk:expr, I32) => {
        matches!($chunk, $crate::AudioChunk::I32(_))
    };
    ($chunk:expr, F32) => {
        matches!($chunk, $crate::AudioChunk::F32(_))
    };
    ($chunk:expr, F64) => {
        matches!($chunk, $crate::AudioChunk::F64(_))
    };
}

/// Extrait un AudioChunk d'un AudioSegment
///
/// # Exemples
/// ```
/// use pmoaudio::{AudioSegment, extract_audio_chunk};
///
/// fn get_chunk(segment: &AudioSegment) -> Option<&Arc<AudioChunk>> {
///     extract_audio_chunk!(segment)
/// }
/// ```
#[macro_export]
macro_rules! extract_audio_chunk {
    ($segment:expr) => {
        match &$segment.segment {
            $crate::_AudioSegment::Chunk(chunk) => Some(chunk),
            _ => None,
        }
    };
}

/// Extrait un SyncMarker d'un AudioSegment
///
/// # Exemples
/// ```
/// use pmoaudio::{AudioSegment, extract_sync_marker};
///
/// fn get_marker(segment: &AudioSegment) -> Option<&Arc<SyncMarker>> {
///     extract_sync_marker!(segment)
/// }
/// ```
#[macro_export]
macro_rules! extract_sync_marker {
    ($segment:expr) => {
        match &$segment.segment {
            $crate::_AudioSegment::Sync(marker) => Some(marker),
            _ => None,
        }
    };
}

/// Match sur le contenu d'un AudioSegment
///
/// # Exemples
/// ```
/// use pmoaudio::{AudioSegment, match_segment};
///
/// fn process_segment(segment: &AudioSegment) {
///     match_segment!(segment,
///         chunk => println!("Audio chunk: {}", chunk.type_name()),
///         marker => println!("Sync marker")
///     );
/// }
/// ```
#[macro_export]
macro_rules! match_segment {
    ($segment:expr, $chunk_name:ident => $chunk_body:expr, $marker_name:ident => $marker_body:expr) => {
        match &$segment.segment {
            $crate::_AudioSegment::Chunk($chunk_name) => $chunk_body,
            $crate::_AudioSegment::Sync($marker_name) => $marker_body,
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::{AudioChunk, AudioChunkData, AudioSegment, BitDepth};

    #[test]
    fn test_extract_chunk_data() {
        let data = AudioChunkData::new(vec![[100i32, 200i32]], 44100, 0.0);
        let chunk = AudioChunk::I32(data.clone());

        // Test extraction réussie
        assert!(extract_chunk_data!(&chunk, I32).is_some());
        assert!(extract_chunk_data!(&chunk, F32).is_none());

        // Test avec F32
        let f32_chunk = AudioChunk::F32(AudioChunkData::new(vec![[0.5f32, -0.5f32]], 44100, 0.0));
        assert!(extract_chunk_data!(&f32_chunk, F32).is_some());
        assert!(extract_chunk_data!(&f32_chunk, I32).is_none());
    }

    #[test]
    fn test_match_chunk() {
        let chunk = AudioChunk::I32(AudioChunkData::new(vec![[100i32, 200i32]], 44100, 0.0));

        let len = match_chunk!(&chunk, data => data.len());
        assert_eq!(len, 1);

        let sample_rate = match_chunk!(&chunk, data => data.get_sample_rate());
        assert_eq!(sample_rate, 44100);
    }

    #[test]
    fn test_map_chunk() {
        let chunk = AudioChunk::I32(AudioChunkData::new(vec![[100i32, 200i32]], 44100, 0.0));

        let modified = map_chunk!(&chunk, data => data.set_gain_db(6.0));

        match_chunk!(&modified, data => {
            assert_eq!(data.get_gain_db(), 6.0);
        });
    }

    #[test]
    fn test_is_chunk_type() {
        let i32_chunk = AudioChunk::I32(AudioChunkData::new(vec![[100i32, 200i32]], 44100, 0.0));
        let f32_chunk = AudioChunk::F32(AudioChunkData::new(vec![[0.5f32, -0.5f32]], 44100, 0.0));

        assert!(is_chunk_type!(&i32_chunk, I32));
        assert!(!is_chunk_type!(&i32_chunk, F32));
        assert!(is_chunk_type!(&f32_chunk, F32));
        assert!(!is_chunk_type!(&f32_chunk, I32));
    }

    #[test]
    fn test_extract_audio_chunk() {
        let segment = AudioSegment::new_chunk(0, 0.0, vec![[100i32, 200i32]], 44100, BitDepth::B32);

        assert!(extract_audio_chunk!(&*segment).is_some());

        let sync_segment = AudioSegment::new_heartbeat(1, 1.0);
        assert!(extract_audio_chunk!(&*sync_segment).is_none());
    }

    #[test]
    fn test_extract_sync_marker() {
        let segment = AudioSegment::new_heartbeat(1, 1.0);
        assert!(extract_sync_marker!(&*segment).is_some());

        let audio_segment =
            AudioSegment::new_chunk(0, 0.0, vec![[100i32, 200i32]], 44100, BitDepth::B32);
        assert!(extract_sync_marker!(&*audio_segment).is_none());
    }

    #[test]
    fn test_match_segment() {
        let audio_segment =
            AudioSegment::new_chunk(0, 0.0, vec![[100i32, 200i32]], 44100, BitDepth::B32);

        let result = match_segment!(&*audio_segment,
            chunk => format!("audio: {}", chunk.type_name()),
            _marker => "sync".to_string()
        );
        assert_eq!(result, "audio: i32");

        let sync_segment = AudioSegment::new_heartbeat(1, 1.0);
        let result = match_segment!(&*sync_segment,
            _chunk => "audio".to_string(),
            _marker => "sync".to_string()
        );
        assert_eq!(result, "sync");
    }
}
