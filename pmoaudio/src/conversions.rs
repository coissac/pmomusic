//! Conversions entre différents types de AudioChunk
//!
//! Ce module fournit des conversions optimisées (SIMD où possible) entre
//! tous les types de samples audio supportés.

use std::sync::Arc;

use crate::{dsp, AudioChunk, AudioChunkData, BitDepth, I24};

// ============================================================================
// Conversions int → int (changement de bit depth)
// ============================================================================
//
// Ces fonctions utilisent la fonction DSP optimisée SIMD `bitdepth_change_stereo`
// pour les conversions i32 ↔ i32 avec différents bit depths.

/// Convertit i32 vers i8 (downsampling via bit depth change)
pub fn convert_i32_to_i8(chunk: &AudioChunkData<i32>) -> Arc<AudioChunkData<i8>> {
    let mut stereo = chunk.clone_frames();

    // Utiliser la fonction DSP optimisée pour passer de B32 → B8
    dsp::bitdepth_change_stereo(&mut stereo, BitDepth::B32, BitDepth::B8);

    // Convertir i32 → i8 (les valeurs sont maintenant dans la plage i8)
    let stereo_i8: Vec<[i8; 2]> = stereo
        .into_iter()
        .map(|[l, r]| [l as i8, r as i8])
        .collect();

    AudioChunkData::new(stereo_i8, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit i32 vers i16 (downsampling via bit depth change)
pub fn convert_i32_to_i16(chunk: &AudioChunkData<i32>) -> Arc<AudioChunkData<i16>> {
    let mut stereo = chunk.clone_frames();

    // Utiliser la fonction DSP optimisée pour passer de B32 → B16
    dsp::bitdepth_change_stereo(&mut stereo, BitDepth::B32, BitDepth::B16);

    // Convertir i32 → i16 (les valeurs sont maintenant dans la plage i16)
    let stereo_i16: Vec<[i16; 2]> = stereo
        .into_iter()
        .map(|[l, r]| [l as i16, r as i16])
        .collect();

    AudioChunkData::new(stereo_i16, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit i32 vers I24 (downsampling via bit depth change)
pub fn convert_i32_to_i24(chunk: &AudioChunkData<i32>) -> Arc<AudioChunkData<I24>> {
    let mut stereo = chunk.clone_frames();

    // Utiliser la fonction DSP optimisée pour passer de B32 → B24
    dsp::bitdepth_change_stereo(&mut stereo, BitDepth::B32, BitDepth::B24);

    // Convertir i32 → I24 (les valeurs sont maintenant dans la plage I24)
    let stereo_i24: Vec<[I24; 2]> = stereo
        .into_iter()
        .map(|[l, r]| [I24::new_clamped(l), I24::new_clamped(r)])
        .collect();

    AudioChunkData::new(stereo_i24, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit i8 vers i32 (upsampling via bit depth change)
pub fn convert_i8_to_i32(chunk: &AudioChunkData<i8>) -> Arc<AudioChunkData<i32>> {
    // Convertir i8 → i32 d'abord
    let mut stereo: Vec<[i32; 2]> = chunk
        .frames()
        .iter()
        .map(|[l, r]| [*l as i32, *r as i32])
        .collect();

    // Utiliser la fonction DSP optimisée pour passer de B8 → B32
    dsp::bitdepth_change_stereo(&mut stereo, BitDepth::B8, BitDepth::B32);

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit i16 vers i32 (upsampling via bit depth change)
pub fn convert_i16_to_i32(chunk: &AudioChunkData<i16>) -> Arc<AudioChunkData<i32>> {
    // Convertir i16 → i32 d'abord
    let mut stereo: Vec<[i32; 2]> = chunk
        .frames()
        .iter()
        .map(|[l, r]| [*l as i32, *r as i32])
        .collect();

    // Utiliser la fonction DSP optimisée pour passer de B16 → B32
    dsp::bitdepth_change_stereo(&mut stereo, BitDepth::B16, BitDepth::B32);

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit I24 vers i32 (upsampling via bit depth change)
pub fn convert_i24_to_i32(chunk: &AudioChunkData<I24>) -> Arc<AudioChunkData<i32>> {
    // Convertir I24 → i32 d'abord
    let mut stereo: Vec<[i32; 2]> = chunk
        .frames()
        .iter()
        .map(|[l, r]| [l.as_i32(), r.as_i32()])
        .collect();

    // Utiliser la fonction DSP optimisée pour passer de B24 → B32
    dsp::bitdepth_change_stereo(&mut stereo, BitDepth::B24, BitDepth::B32);

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

// ============================================================================
// Conversions int → float (normalisation)
// ============================================================================

/// Convertit i32 vers f32 via les fonctions DSP optimisées SIMD
///
/// I32 = 32 bits complets, donc normalisation par 2^31
pub fn convert_i32_to_f32(chunk: &AudioChunkData<i32>) -> Arc<AudioChunkData<f32>> {
    let frames = chunk.frames();
    let len = frames.len();

    // Séparer les canaux pour utiliser les fonctions DSP SIMD
    let mut left = Vec::with_capacity(len);
    let mut right = Vec::with_capacity(len);
    for [l, r] in frames {
        left.push(*l);
        right.push(*r);
    }

    // Utiliser la fonction SIMD optimisée du module DSP avec BitDepth::B32
    let mut out_pairs = vec![[0.0f32; 2]; len];
    dsp::i32_stereo_to_pairs_f32(&left, &right, &mut out_pairs, BitDepth::B32);

    AudioChunkData::new(out_pairs, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit i32 vers f64
///
/// I32 = 32 bits complets, donc normalisation par 2^31
pub fn convert_i32_to_f64(chunk: &AudioChunkData<i32>) -> Arc<AudioChunkData<f64>> {
    // Via f32 puis upcast
    let f32_chunk = convert_i32_to_f32(chunk);
    convert_f32_to_f64(&f32_chunk)
}

/// Convertit I24 vers f32
pub fn convert_i24_to_f32(chunk: &AudioChunkData<I24>) -> Arc<AudioChunkData<f32>> {
    let frames = chunk.frames();
    let max_value = 8_388_608.0f32; // 2^23

    let stereo: Vec<[f32; 2]> = frames
        .iter()
        .map(|[l, r]| {
            let lf = l.as_i32() as f32 / max_value;
            let rf = r.as_i32() as f32 / max_value;
            [lf, rf]
        })
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit I24 vers f64
pub fn convert_i24_to_f64(chunk: &AudioChunkData<I24>) -> Arc<AudioChunkData<f64>> {
    let frames = chunk.frames();
    let max_value = 8_388_608.0f64; // 2^23

    let stereo: Vec<[f64; 2]> = frames
        .iter()
        .map(|[l, r]| {
            let lf = l.as_i32() as f64 / max_value;
            let rf = r.as_i32() as f64 / max_value;
            [lf, rf]
        })
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit i16 vers f32
pub fn convert_i16_to_f32(chunk: &AudioChunkData<i16>) -> Arc<AudioChunkData<f32>> {
    let frames = chunk.frames();
    let max_value = 32_768.0f32; // 2^15

    let stereo: Vec<[f32; 2]> = frames
        .iter()
        .map(|[l, r]| {
            let lf = *l as f32 / max_value;
            let rf = *r as f32 / max_value;
            [lf, rf]
        })
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit i16 vers f64
pub fn convert_i16_to_f64(chunk: &AudioChunkData<i16>) -> Arc<AudioChunkData<f64>> {
    let frames = chunk.frames();
    let max_value = 32_768.0f64; // 2^15

    let stereo: Vec<[f64; 2]> = frames
        .iter()
        .map(|[l, r]| {
            let lf = *l as f64 / max_value;
            let rf = *r as f64 / max_value;
            [lf, rf]
        })
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit i8 vers f32
pub fn convert_i8_to_f32(chunk: &AudioChunkData<i8>) -> Arc<AudioChunkData<f32>> {
    let frames = chunk.frames();
    let max_value = 128.0f32; // 2^7

    let stereo: Vec<[f32; 2]> = frames
        .iter()
        .map(|[l, r]| {
            let lf = *l as f32 / max_value;
            let rf = *r as f32 / max_value;
            [lf, rf]
        })
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit i8 vers f64
pub fn convert_i8_to_f64(chunk: &AudioChunkData<i8>) -> Arc<AudioChunkData<f64>> {
    let frames = chunk.frames();
    let max_value = 128.0f64; // 2^7

    let stereo: Vec<[f64; 2]> = frames
        .iter()
        .map(|[l, r]| {
            let lf = *l as f64 / max_value;
            let rf = *r as f64 / max_value;
            [lf, rf]
        })
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

// ============================================================================
// Conversions float → int (quantization)
// ============================================================================

/// Convertit f32 vers i32 via les fonctions DSP optimisées SIMD
///
/// I32 = 32 bits complets, donc quantization vers ±2^31
pub fn convert_f32_to_i32(chunk: &AudioChunkData<f32>) -> Arc<AudioChunkData<i32>> {
    let frames = chunk.frames();
    let len = frames.len();

    // Utiliser la fonction SIMD optimisée du module DSP avec BitDepth::B32
    let mut left = vec![0i32; len];
    let mut right = vec![0i32; len];
    dsp::pairs_f32_to_i32_stereo(frames, &mut left, &mut right, BitDepth::B32);

    // Recombiner en frames
    let stereo: Vec<[i32; 2]> = left
        .into_iter()
        .zip(right.into_iter())
        .map(|(l, r)| [l, r])
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit f64 vers i32 (via f32)
///
/// I32 = 32 bits complets, donc quantization vers ±2^31
pub fn convert_f64_to_i32(chunk: &AudioChunkData<f64>) -> Arc<AudioChunkData<i32>> {
    // Downcast f64 → f32 puis quantize
    let f32_chunk = convert_f64_to_f32(chunk);
    convert_f32_to_i32(&f32_chunk)
}

/// Convertit f32 vers I24
pub fn convert_f32_to_i24(chunk: &AudioChunkData<f32>) -> Arc<AudioChunkData<I24>> {
    let frames = chunk.frames();
    let max_value = 8_388_607.0f32; // 2^23 - 1
    let min_value = -8_388_608.0f32; // -2^23

    let stereo: Vec<[I24; 2]> = frames
        .iter()
        .map(|[l, r]| {
            let l_scaled = (l * max_value).clamp(min_value, max_value).round() as i32;
            let r_scaled = (r * max_value).clamp(min_value, max_value).round() as i32;
            [I24::new_clamped(l_scaled), I24::new_clamped(r_scaled)]
        })
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit f64 vers I24
pub fn convert_f64_to_i24(chunk: &AudioChunkData<f64>) -> Arc<AudioChunkData<I24>> {
    let frames = chunk.frames();
    let max_value = 8_388_607.0f64; // 2^23 - 1
    let min_value = -8_388_608.0f64; // -2^23

    let stereo: Vec<[I24; 2]> = frames
        .iter()
        .map(|[l, r]| {
            let l_scaled = (l * max_value).clamp(min_value, max_value).round() as i32;
            let r_scaled = (r * max_value).clamp(min_value, max_value).round() as i32;
            [I24::new_clamped(l_scaled), I24::new_clamped(r_scaled)]
        })
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit f32 vers i16
pub fn convert_f32_to_i16(chunk: &AudioChunkData<f32>) -> Arc<AudioChunkData<i16>> {
    let frames = chunk.frames();
    let max_value = 32_767.0f32; // 2^15 - 1
    let min_value = -32_768.0f32; // -2^15

    let stereo: Vec<[i16; 2]> = frames
        .iter()
        .map(|[l, r]| {
            let l16 = (l * max_value).clamp(min_value, max_value).round() as i16;
            let r16 = (r * max_value).clamp(min_value, max_value).round() as i16;
            [l16, r16]
        })
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit f64 vers i16
pub fn convert_f64_to_i16(chunk: &AudioChunkData<f64>) -> Arc<AudioChunkData<i16>> {
    let frames = chunk.frames();
    let max_value = 32_767.0f64; // 2^15 - 1
    let min_value = -32_768.0f64; // -2^15

    let stereo: Vec<[i16; 2]> = frames
        .iter()
        .map(|[l, r]| {
            let l16 = (l * max_value).clamp(min_value, max_value).round() as i16;
            let r16 = (r * max_value).clamp(min_value, max_value).round() as i16;
            [l16, r16]
        })
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit f32 vers i8
pub fn convert_f32_to_i8(chunk: &AudioChunkData<f32>) -> Arc<AudioChunkData<i8>> {
    let frames = chunk.frames();
    let max_value = 127.0f32; // 2^7 - 1
    let min_value = -128.0f32; // -2^7

    let stereo: Vec<[i8; 2]> = frames
        .iter()
        .map(|[l, r]| {
            let l8 = (l * max_value).clamp(min_value, max_value).round() as i8;
            let r8 = (r * max_value).clamp(min_value, max_value).round() as i8;
            [l8, r8]
        })
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit f64 vers i8
pub fn convert_f64_to_i8(chunk: &AudioChunkData<f64>) -> Arc<AudioChunkData<i8>> {
    let frames = chunk.frames();
    let max_value = 127.0f64; // 2^7 - 1
    let min_value = -128.0f64; // -2^7

    let stereo: Vec<[i8; 2]> = frames
        .iter()
        .map(|[l, r]| {
            let l8 = (l * max_value).clamp(min_value, max_value).round() as i8;
            let r8 = (r * max_value).clamp(min_value, max_value).round() as i8;
            [l8, r8]
        })
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

// ============================================================================
// Conversions F32 ↔ F64
// ============================================================================

/// Convertit f32 vers f64 (upcast simple)
pub fn convert_f32_to_f64(chunk: &AudioChunkData<f32>) -> Arc<AudioChunkData<f64>> {
    let frames = chunk.frames();
    let stereo: Vec<[f64; 2]> = frames
        .iter()
        .map(|[l, r]| [*l as f64, *r as f64])
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

/// Convertit f64 vers f32 (downcast simple)
pub fn convert_f64_to_f32(chunk: &AudioChunkData<f64>) -> Arc<AudioChunkData<f32>> {
    let frames = chunk.frames();
    let stereo: Vec<[f32; 2]> = frames
        .iter()
        .map(|[l, r]| [*l as f32, *r as f32])
        .collect();

    AudioChunkData::new(stereo, chunk.sample_rate(), chunk.gain_db())
}

// ============================================================================
// Méthodes de conversion sur AudioChunk enum
// ============================================================================

impl AudioChunk {
    /// Convertit ce chunk vers f32
    ///
    /// Chaque type utilise sa plage native (I8=±2^7, I16=±2^15, I24=±2^23, I32=±2^31)
    pub fn to_f32(&self) -> AudioChunk {
        match self {
            AudioChunk::I8(d) => AudioChunk::F32(convert_i8_to_f32(d)),
            AudioChunk::I16(d) => AudioChunk::F32(convert_i16_to_f32(d)),
            AudioChunk::I24(d) => AudioChunk::F32(convert_i24_to_f32(d)),
            AudioChunk::I32(d) => AudioChunk::F32(convert_i32_to_f32(d)),
            AudioChunk::F32(d) => AudioChunk::F32(d.clone()),
            AudioChunk::F64(d) => AudioChunk::F32(convert_f64_to_f32(d)),
        }
    }

    /// Convertit ce chunk vers f64
    ///
    /// Chaque type utilise sa plage native (I8=±2^7, I16=±2^15, I24=±2^23, I32=±2^31)
    pub fn to_f64(&self) -> AudioChunk {
        match self {
            AudioChunk::I8(d) => AudioChunk::F64(convert_i8_to_f64(d)),
            AudioChunk::I16(d) => AudioChunk::F64(convert_i16_to_f64(d)),
            AudioChunk::I24(d) => AudioChunk::F64(convert_i24_to_f64(d)),
            AudioChunk::I32(d) => AudioChunk::F64(convert_i32_to_f64(d)),
            AudioChunk::F32(d) => AudioChunk::F64(convert_f32_to_f64(d)),
            AudioChunk::F64(d) => AudioChunk::F64(d.clone()),
        }
    }

    /// Convertit ce chunk vers i32
    ///
    /// I32 = 32 bits complets (±2^31)
    pub fn to_i32(&self) -> AudioChunk {
        match self {
            AudioChunk::I8(d) => AudioChunk::I32(convert_i8_to_i32(d)),
            AudioChunk::I16(d) => AudioChunk::I32(convert_i16_to_i32(d)),
            AudioChunk::I24(d) => AudioChunk::I32(convert_i24_to_i32(d)),
            AudioChunk::I32(d) => AudioChunk::I32(d.clone()),
            AudioChunk::F32(d) => AudioChunk::I32(convert_f32_to_i32(d)),
            AudioChunk::F64(d) => AudioChunk::I32(convert_f64_to_i32(d)),
        }
    }

    /// Convertit ce chunk vers I24
    pub fn to_i24(&self) -> AudioChunk {
        match self {
            AudioChunk::I8(d) => {
                // I8 → I32 → I24
                let i32_chunk = convert_i8_to_i32(d);
                AudioChunk::I24(convert_i32_to_i24(&i32_chunk))
            }
            AudioChunk::I16(d) => {
                // I16 → I32 → I24
                let i32_chunk = convert_i16_to_i32(d);
                AudioChunk::I24(convert_i32_to_i24(&i32_chunk))
            }
            AudioChunk::I24(d) => AudioChunk::I24(d.clone()),
            AudioChunk::I32(d) => AudioChunk::I24(convert_i32_to_i24(d)),
            AudioChunk::F32(d) => AudioChunk::I24(convert_f32_to_i24(d)),
            AudioChunk::F64(d) => AudioChunk::I24(convert_f64_to_i24(d)),
        }
    }

    /// Convertit ce chunk vers i16
    pub fn to_i16(&self) -> AudioChunk {
        match self {
            AudioChunk::I8(d) => {
                // I8 → I32 → I16
                let i32_chunk = convert_i8_to_i32(d);
                AudioChunk::I16(convert_i32_to_i16(&i32_chunk))
            }
            AudioChunk::I16(d) => AudioChunk::I16(d.clone()),
            AudioChunk::I24(d) => {
                // I24 → I32 → I16
                let i32_chunk = convert_i24_to_i32(d);
                AudioChunk::I16(convert_i32_to_i16(&i32_chunk))
            }
            AudioChunk::I32(d) => AudioChunk::I16(convert_i32_to_i16(d)),
            AudioChunk::F32(d) => AudioChunk::I16(convert_f32_to_i16(d)),
            AudioChunk::F64(d) => AudioChunk::I16(convert_f64_to_i16(d)),
        }
    }

    /// Convertit ce chunk vers i8
    pub fn to_i8(&self) -> AudioChunk {
        match self {
            AudioChunk::I8(d) => AudioChunk::I8(d.clone()),
            AudioChunk::I16(d) => {
                // I16 → I32 → I8
                let i32_chunk = convert_i16_to_i32(d);
                AudioChunk::I8(convert_i32_to_i8(&i32_chunk))
            }
            AudioChunk::I24(d) => {
                // I24 → I32 → I8
                let i32_chunk = convert_i24_to_i32(d);
                AudioChunk::I8(convert_i32_to_i8(&i32_chunk))
            }
            AudioChunk::I32(d) => AudioChunk::I8(convert_i32_to_i8(d)),
            AudioChunk::F32(d) => AudioChunk::I8(convert_f32_to_i8(d)),
            AudioChunk::F64(d) => AudioChunk::I8(convert_f64_to_i8(d)),
        }
    }
}

// ============================================================================
// Implémentations des traits From/Into
// ============================================================================

// ---------- From<Arc<AudioChunkData<T>>> pour AudioChunk ----------

impl From<Arc<AudioChunkData<i8>>> for AudioChunk {
    fn from(data: Arc<AudioChunkData<i8>>) -> Self {
        AudioChunk::I8(data)
    }
}

impl From<Arc<AudioChunkData<i16>>> for AudioChunk {
    fn from(data: Arc<AudioChunkData<i16>>) -> Self {
        AudioChunk::I16(data)
    }
}

impl From<Arc<AudioChunkData<I24>>> for AudioChunk {
    fn from(data: Arc<AudioChunkData<I24>>) -> Self {
        AudioChunk::I24(data)
    }
}

impl From<Arc<AudioChunkData<i32>>> for AudioChunk {
    fn from(data: Arc<AudioChunkData<i32>>) -> Self {
        AudioChunk::I32(data)
    }
}

impl From<Arc<AudioChunkData<f32>>> for AudioChunk {
    fn from(data: Arc<AudioChunkData<f32>>) -> Self {
        AudioChunk::F32(data)
    }
}

impl From<Arc<AudioChunkData<f64>>> for AudioChunk {
    fn from(data: Arc<AudioChunkData<f64>>) -> Self {
        AudioChunk::F64(data)
    }
}

// ---------- From entre AudioChunkData types (sans BitDepth requis) ----------

// I8 conversions
impl From<&AudioChunkData<i8>> for Arc<AudioChunkData<i32>> {
    fn from(chunk: &AudioChunkData<i8>) -> Self {
        convert_i8_to_i32(chunk)
    }
}

impl From<&AudioChunkData<i8>> for Arc<AudioChunkData<f32>> {
    fn from(chunk: &AudioChunkData<i8>) -> Self {
        convert_i8_to_f32(chunk)
    }
}

impl From<&AudioChunkData<i8>> for Arc<AudioChunkData<f64>> {
    fn from(chunk: &AudioChunkData<i8>) -> Self {
        convert_i8_to_f64(chunk)
    }
}

// I16 conversions
impl From<&AudioChunkData<i16>> for Arc<AudioChunkData<i32>> {
    fn from(chunk: &AudioChunkData<i16>) -> Self {
        convert_i16_to_i32(chunk)
    }
}

impl From<&AudioChunkData<i16>> for Arc<AudioChunkData<f32>> {
    fn from(chunk: &AudioChunkData<i16>) -> Self {
        convert_i16_to_f32(chunk)
    }
}

impl From<&AudioChunkData<i16>> for Arc<AudioChunkData<f64>> {
    fn from(chunk: &AudioChunkData<i16>) -> Self {
        convert_i16_to_f64(chunk)
    }
}

// I24 conversions
impl From<&AudioChunkData<I24>> for Arc<AudioChunkData<i32>> {
    fn from(chunk: &AudioChunkData<I24>) -> Self {
        convert_i24_to_i32(chunk)
    }
}

impl From<&AudioChunkData<I24>> for Arc<AudioChunkData<f32>> {
    fn from(chunk: &AudioChunkData<I24>) -> Self {
        convert_i24_to_f32(chunk)
    }
}

impl From<&AudioChunkData<I24>> for Arc<AudioChunkData<f64>> {
    fn from(chunk: &AudioChunkData<I24>) -> Self {
        convert_i24_to_f64(chunk)
    }
}

// I32 conversions vers types int (downsampling)
impl From<&AudioChunkData<i32>> for Arc<AudioChunkData<i8>> {
    fn from(chunk: &AudioChunkData<i32>) -> Self {
        convert_i32_to_i8(chunk)
    }
}

impl From<&AudioChunkData<i32>> for Arc<AudioChunkData<i16>> {
    fn from(chunk: &AudioChunkData<i32>) -> Self {
        convert_i32_to_i16(chunk)
    }
}

impl From<&AudioChunkData<i32>> for Arc<AudioChunkData<I24>> {
    fn from(chunk: &AudioChunkData<i32>) -> Self {
        convert_i32_to_i24(chunk)
    }
}

// I32 conversions vers float (normalisation par 2^31)
impl From<&AudioChunkData<i32>> for Arc<AudioChunkData<f32>> {
    fn from(chunk: &AudioChunkData<i32>) -> Self {
        convert_i32_to_f32(chunk)
    }
}

impl From<&AudioChunkData<i32>> for Arc<AudioChunkData<f64>> {
    fn from(chunk: &AudioChunkData<i32>) -> Self {
        convert_i32_to_f64(chunk)
    }
}

// F32 conversions
impl From<&AudioChunkData<f32>> for Arc<AudioChunkData<f64>> {
    fn from(chunk: &AudioChunkData<f32>) -> Self {
        convert_f32_to_f64(chunk)
    }
}

impl From<&AudioChunkData<f32>> for Arc<AudioChunkData<i8>> {
    fn from(chunk: &AudioChunkData<f32>) -> Self {
        convert_f32_to_i8(chunk)
    }
}

impl From<&AudioChunkData<f32>> for Arc<AudioChunkData<i16>> {
    fn from(chunk: &AudioChunkData<f32>) -> Self {
        convert_f32_to_i16(chunk)
    }
}

impl From<&AudioChunkData<f32>> for Arc<AudioChunkData<I24>> {
    fn from(chunk: &AudioChunkData<f32>) -> Self {
        convert_f32_to_i24(chunk)
    }
}

impl From<&AudioChunkData<f32>> for Arc<AudioChunkData<i32>> {
    fn from(chunk: &AudioChunkData<f32>) -> Self {
        convert_f32_to_i32(chunk)
    }
}

// F64 conversions
impl From<&AudioChunkData<f64>> for Arc<AudioChunkData<f32>> {
    fn from(chunk: &AudioChunkData<f64>) -> Self {
        convert_f64_to_f32(chunk)
    }
}

impl From<&AudioChunkData<f64>> for Arc<AudioChunkData<i8>> {
    fn from(chunk: &AudioChunkData<f64>) -> Self {
        convert_f64_to_i8(chunk)
    }
}

impl From<&AudioChunkData<f64>> for Arc<AudioChunkData<i16>> {
    fn from(chunk: &AudioChunkData<f64>) -> Self {
        convert_f64_to_i16(chunk)
    }
}

impl From<&AudioChunkData<f64>> for Arc<AudioChunkData<I24>> {
    fn from(chunk: &AudioChunkData<f64>) -> Self {
        convert_f64_to_i24(chunk)
    }
}

impl From<&AudioChunkData<f64>> for Arc<AudioChunkData<i32>> {
    fn from(chunk: &AudioChunkData<f64>) -> Self {
        convert_f64_to_i32(chunk)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i32_to_f32_roundtrip() {
        let stereo = vec![[1_000_000_000i32, 2_000_000_000i32]; 100];
        let chunk_i32 = AudioChunkData::new(stereo.clone(), 48_000, 0.0);

        let chunk_f32 = convert_i32_to_f32(&chunk_i32);
        let chunk_back = convert_f32_to_i32(&chunk_f32);

        // Vérifier que les valeurs sont proches (tolérance d'arrondi)
        // Note: Pour I32 on utilise toute la plage ±2^31
        for (orig, back) in stereo.iter().zip(chunk_back.frames().iter()) {
            assert!((orig[0] - back[0]).abs() <= 100); // Tolérance plus élevée pour 32-bit
            assert!((orig[1] - back[1]).abs() <= 100);
        }
    }

    #[test]
    fn test_f32_to_f64_roundtrip() {
        let stereo = vec![[0.5f32, -0.25f32]; 100];
        let chunk_f32 = AudioChunkData::new(stereo.clone(), 48_000, 0.0);

        let chunk_f64 = convert_f32_to_f64(&chunk_f32);
        let chunk_back = convert_f64_to_f32(&chunk_f64);

        // Vérifier égalité exacte (pas de perte de précision significative)
        for (orig, back) in stereo.iter().zip(chunk_back.frames().iter()) {
            assert!((orig[0] - back[0]).abs() < 1e-6);
            assert!((orig[1] - back[1]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_i16_to_i32_upsampling() {
        let stereo = vec![[16_000i16, -8_000i16]; 10];
        let chunk_i16 = AudioChunkData::new(stereo.clone(), 48_000, 0.0);

        let chunk_i32 = convert_i16_to_i32(&chunk_i16);

        // Vérifier que les valeurs sont correctement upsamplées (shift de 16 bits)
        for (orig, result) in stereo.iter().zip(chunk_i32.frames().iter()) {
            assert_eq!(result[0], (orig[0] as i32) << 16);
            assert_eq!(result[1], (orig[1] as i32) << 16);
        }
    }

    #[test]
    fn test_i32_to_i16_downsampling() {
        let stereo = vec![[1_000_000i32 << 16, -500_000i32 << 16]; 10];
        let chunk_i32 = AudioChunkData::new(stereo.clone(), 48_000, 0.0);

        let chunk_i16 = convert_i32_to_i16(&chunk_i32);

        // Vérifier que les valeurs sont correctement downsamplées
        for (orig, result) in stereo.iter().zip(chunk_i16.frames().iter()) {
            assert_eq!(result[0], (orig[0] >> 16) as i16);
            assert_eq!(result[1], (orig[1] >> 16) as i16);
        }
    }

    #[test]
    fn test_i24_conversions() {
        let stereo = vec![
            [I24::new(1_000_000).unwrap(), I24::new(-500_000).unwrap()];
            10
        ];
        let chunk_i24 = AudioChunkData::new(stereo.clone(), 48_000, 0.0);

        // I24 → F32 → I24
        let chunk_f32 = convert_i24_to_f32(&chunk_i24);
        let chunk_back = convert_f32_to_i24(&chunk_f32);

        for (orig, back) in stereo.iter().zip(chunk_back.frames().iter()) {
            assert!((orig[0].as_i32() - back[0].as_i32()).abs() <= 1);
            assert!((orig[1].as_i32() - back[1].as_i32()).abs() <= 1);
        }
    }

    #[test]
    fn test_audio_chunk_enum_conversions() {
        // Créer un chunk I32
        let stereo = vec![[1_000_000_000i32, -500_000_000i32]; 100];
        let chunk_data = AudioChunkData::new(stereo, 48_000, 0.0);
        let chunk = AudioChunk::I32(chunk_data);

        // Convertir vers F32 (I32 utilise plage complète ±2^31)
        let chunk_f32 = chunk.to_f32();
        assert_eq!(chunk_f32.type_name(), "f32");

        // Convertir vers I24
        let chunk_i24 = chunk.to_i24();
        assert_eq!(chunk_i24.type_name(), "I24");

        // Convertir vers I16
        let chunk_i16 = chunk.to_i16();
        assert_eq!(chunk_i16.type_name(), "i16");
    }

    #[test]
    fn test_from_trait_audio_chunk() {
        // Test From<Arc<AudioChunkData<T>>> pour AudioChunk
        let stereo_f32 = vec![[0.5f32, -0.25f32]; 100];
        let chunk_data = AudioChunkData::new(stereo_f32, 48_000, 0.0);

        // Utiliser From/Into
        let chunk: AudioChunk = chunk_data.into();
        assert_eq!(chunk.type_name(), "f32");
        assert_eq!(chunk.len(), 100);
    }

    #[test]
    fn test_from_trait_conversions() {
        // Test From entre AudioChunkData types
        let stereo_i16 = vec![[16_000i16, -8_000i16]; 50];
        let chunk_i16 = AudioChunkData::new(stereo_i16, 48_000, 0.0);

        // I16 → I32 via From
        let chunk_i32: Arc<AudioChunkData<i32>> = (&*chunk_i16).into();
        assert_eq!(chunk_i32.len(), 50);

        // I16 → F32 via From
        let chunk_f32: Arc<AudioChunkData<f32>> = (&*chunk_i16).into();
        assert_eq!(chunk_f32.len(), 50);

        // I16 → F64 via From
        let chunk_f64: Arc<AudioChunkData<f64>> = (&*chunk_i16).into();
        assert_eq!(chunk_f64.len(), 50);
    }

    #[test]
    fn test_from_trait_i24() {
        // Test conversions I24 via From
        let stereo_i24 = vec![
            [I24::new(1_000_000).unwrap(), I24::new(-500_000).unwrap()];
            50
        ];
        let chunk_i24 = AudioChunkData::new(stereo_i24, 48_000, 0.0);

        // I24 → I32 via From
        let chunk_i32: Arc<AudioChunkData<i32>> = (&*chunk_i24).into();
        assert_eq!(chunk_i32.len(), 50);

        // I24 → F32 via From
        let chunk_f32: Arc<AudioChunkData<f32>> = (&*chunk_i24).into();
        assert_eq!(chunk_f32.len(), 50);
    }

    #[test]
    fn test_from_trait_float_conversions() {
        // Test conversions float via From
        let stereo_f32 = vec![[0.5f32, -0.25f32]; 50];
        let chunk_f32 = AudioChunkData::new(stereo_f32, 48_000, 0.0);

        // F32 → F64 via From
        let chunk_f64: Arc<AudioChunkData<f64>> = (&*chunk_f32).into();
        assert_eq!(chunk_f64.len(), 50);

        // F32 → I16 via From
        let chunk_i16: Arc<AudioChunkData<i16>> = (&*chunk_f32).into();
        assert_eq!(chunk_i16.len(), 50);

        // F32 → I24 via From
        let chunk_i24: Arc<AudioChunkData<I24>> = (&*chunk_f32).into();
        assert_eq!(chunk_i24.len(), 50);
    }

    #[test]
    fn test_from_trait_roundtrip() {
        // Test round-trip I24 → F32 → I24 via From
        let original = vec![
            [I24::new(1_000_000).unwrap(), I24::new(-500_000).unwrap()];
            10
        ];
        let chunk_i24 = AudioChunkData::new(original.clone(), 48_000, 0.0);

        // I24 → F32 via From
        let chunk_f32: Arc<AudioChunkData<f32>> = (&*chunk_i24).into();

        // F32 → I24 via From
        let chunk_back: Arc<AudioChunkData<I24>> = (&*chunk_f32).into();

        // Vérifier la précision
        for (orig, back) in original.iter().zip(chunk_back.frames().iter()) {
            assert!((orig[0].as_i32() - back[0].as_i32()).abs() <= 1);
            assert!((orig[1].as_i32() - back[1].as_i32()).abs() <= 1);
        }
    }

    #[test]
    fn test_from_trait_i32_conversions() {
        // Test conversions I32 via From (maintenant disponibles!)
        let stereo_i32 = vec![[1_000_000_000i32, -500_000_000i32]; 50];
        let chunk_i32 = AudioChunkData::new(stereo_i32, 48_000, 0.0);

        // I32 → F32 via From (normalisation par 2^31)
        let chunk_f32: Arc<AudioChunkData<f32>> = (&*chunk_i32).into();
        assert_eq!(chunk_f32.len(), 50);

        // I32 → F64 via From
        let chunk_f64: Arc<AudioChunkData<f64>> = (&*chunk_i32).into();
        assert_eq!(chunk_f64.len(), 50);

        // F32 → I32 via From (quantization vers 2^31)
        let chunk_back_i32: Arc<AudioChunkData<i32>> = (&*chunk_f32).into();
        assert_eq!(chunk_back_i32.len(), 50);
    }
}
