//! AudioChunk : Représentation générique de données audio stéréo
//!
//! Cette nouvelle architecture supporte différents types de samples :
//! - Entiers : i8, i16, I24 (24-bit), i32
//! - Flottants : f32, f64
//!
//! L'utilisation de génériques permet de factoriser le code tout en gardant
//! des performances optimales grâce à la monomorphisation.

use std::sync::Arc;

use crate::{dsp, BitDepth, Sample, I24};

// ============================================================================
// AudioChunkData<T> : Structure générique pour un chunk audio typé
// ============================================================================

/// Représente un chunk audio stéréo typé avec partage zero-copy via Arc
///
/// Cette structure générique encapsule des données audio de n'importe quel type
/// de sample (i8, i16, I24, i32, f32, f64). Les données sont partagées via `Arc`
/// pour permettre un partage efficace entre plusieurs consumers sans copier.
///
/// # Optimisation zero-copy
///
/// - Le clonage d'un `AudioChunkData` ne clone que le pointeur Arc (très rapide)
/// - Les données audio réelles ne sont jamais copiées tant qu'on ne modifie pas
/// - Plusieurs nodes peuvent partager le même chunk simultanément
///
/// # Gain
///
/// Le gain est stocké en décibels (dB) et n'est pas appliqué aux données tant
/// qu'on n'appelle pas explicitement `apply_gain()`. Cela permet de propager
/// des changements de gain sans recopier les données.
///
/// # Exemples
///
/// ```
/// use pmoaudio::{AudioChunkData, I24};
///
/// // Créer un chunk I24
/// let stereo = vec![[I24::new(1_000_000).unwrap(), I24::new(500_000).unwrap()]; 1000];
/// let chunk = AudioChunkData::new(stereo, 48_000, 0.0);
///
/// assert_eq!(chunk.len(), 1000);
/// assert_eq!(chunk.sample_rate(), 48_000);
/// ```
#[derive(Debug, Clone)]
pub struct AudioChunkData<T: Sample> {
    /// Frames stéréo [L, R], partagées et immuables via Arc
    stereo: Arc<[[T; 2]]>,

    /// Taux d'échantillonnage en Hz (44100, 48000, 96000, 192000, etc.)
    sample_rate: u32,

    /// Gain appliqué au flux audio, en décibels (dB)
    ///
    /// Conversion : `gain_linear = 10^(gain_db / 20)`
    /// Valeur par défaut : `0.0 dB` (aucune modification)
    /// Exemples : `-6 dB` ≈ moitié du volume ; `+6 dB` ≈ double
    gain_db: f64,
}

impl<T: Sample> AudioChunkData<T> {
    /// Crée un nouveau chunk audio
    ///
    /// Les vecteurs sont automatiquement wrappés dans `Arc`.
    ///
    /// # Arguments
    ///
    /// * `stereo` - Frames stéréo `[L, R]`
    /// * `sample_rate` - Taux d'échantillonnage en Hz
    /// * `gain_db` - Gain initial en décibels (0.0 = unity gain)
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoaudio::AudioChunkData;
    ///
    /// let chunk = AudioChunkData::new(
    ///     vec![[0.0f32, 0.0f32]; 1000],
    ///     48_000,
    ///     0.0,
    /// );
    /// ```
    pub fn new(stereo: Vec<[T; 2]>, sample_rate: u32, gain_db: f64) -> Arc<Self> {
        Arc::new(Self {
            stereo: Arc::from(stereo),
            sample_rate,
            gain_db,
        })
    }

    /// Retourne le nombre d'échantillons par canal (frames)
    #[inline]
    pub fn len(&self) -> usize {
        self.stereo.len()
    }

    /// Vérifie si le chunk est vide
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.stereo.is_empty()
    }

    /// Taux d'échantillonnage (Hz)
    #[inline]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Gain courant en décibels
    #[inline]
    pub fn gain_db(&self) -> f64 {
        self.gain_db
    }

    /// Gain sous forme linéaire
    #[inline]
    pub fn gain_linear(&self) -> f64 {
        db_to_linear(self.gain_db)
    }

    /// Retourne une vue immuable sur les frames `[L, R]`
    #[inline]
    pub fn frames(&self) -> &[[T; 2]] {
        &self.stereo
    }

    /// Clone les frames stéréo dans un `Vec`
    #[inline]
    pub fn clone_frames(&self) -> Vec<[T; 2]> {
        self.stereo.to_vec()
    }

    /// Définit le gain (retourne un nouveau chunk avec le même Arc mais gain différent)
    ///
    /// Cette méthode est très peu coûteuse car elle ne clone que la structure, pas les données audio.
    pub fn set_gain_db(&self, gain_db: f64) -> Arc<Self> {
        Arc::new(Self {
            stereo: self.stereo.clone(),
            sample_rate: self.sample_rate,
            gain_db,
        })
    }

    /// Définit le gain à l'aide d'un facteur linéaire (>0)
    pub fn set_gain_linear(&self, gain_linear: f64) -> Arc<Self> {
        self.set_gain_db(linear_to_db(gain_linear))
    }

    /// Modifie le gain de ce chunk (ajoute un delta en dB)
    pub fn with_modified_gain_db(&self, delta_gain_db: f64) -> Arc<Self> {
        self.set_gain_db(self.gain_db + delta_gain_db)
    }

    /// Modifie le gain via un facteur linéaire multiplié au gain courant
    pub fn with_modified_gain_linear(&self, gain_linear: f64) -> Arc<Self> {
        self.with_modified_gain_db(linear_to_db(gain_linear))
    }
}

// Méthodes spécifiques pour les types entiers (i8, i16, I24, i32)
impl AudioChunkData<i32> {
    /// Applique le gain et retourne un nouveau chunk avec les données modifiées
    ///
    /// Cette méthode crée un nouveau chunk avec les samples multipliés par le gain.
    /// Le gain du chunk résultant est remis à 0.0 dB.
    pub fn apply_gain(self: Arc<Self>) -> Arc<Self> {
        if self.gain_db.abs() < f64::EPSILON {
            return self; // Pas de gain à appliquer
        }

        let mut stereo = self.clone_frames();
        dsp::apply_gain_stereo(&mut stereo, self.gain_db);

        AudioChunkData::new(stereo, self.sample_rate, 0.0)
    }

    /// Construit un chunk depuis deux vecteurs `i32` séparés (L/R)
    pub fn from_channels(left: Vec<i32>, right: Vec<i32>, sample_rate: u32) -> Arc<Self> {
        assert_eq!(left.len(), right.len(), "channels must have identical length");
        let stereo = left
            .into_iter()
            .zip(right.into_iter())
            .map(|(l, r)| [l, r])
            .collect();
        AudioChunkData::new(stereo, sample_rate, 0.0)
    }

    /// Change la profondeur de bits (bit depth conversion)
    pub fn set_bit_depth(self: Arc<Self>, old_depth: BitDepth, new_depth: BitDepth) -> Arc<Self> {
        if old_depth == new_depth {
            return self;
        }

        let mut stereo = self.clone_frames();
        dsp::bitdepth_change_stereo(&mut stereo, old_depth, new_depth);

        Arc::new(Self {
            stereo: Arc::from(stereo),
            sample_rate: self.sample_rate,
            gain_db: self.gain_db,
        })
    }
}

// Méthodes spécifiques pour f32
impl AudioChunkData<f32> {
    /// Applique le gain et retourne un nouveau chunk avec les données modifiées
    pub fn apply_gain(self: Arc<Self>) -> Arc<Self> {
        if self.gain_db.abs() < f64::EPSILON {
            return self; // Pas de gain à appliquer
        }

        let gain_linear = db_to_linear(self.gain_db) as f32;
        let mut stereo = self.clone_frames();
        for frame in &mut stereo {
            frame[0] *= gain_linear;
            frame[1] *= gain_linear;
        }

        AudioChunkData::new(stereo, self.sample_rate, 0.0)
    }

    /// Construit un chunk depuis deux vecteurs `f32` séparés (L/R)
    pub fn from_channels(left: Vec<f32>, right: Vec<f32>, sample_rate: u32) -> Arc<Self> {
        assert_eq!(left.len(), right.len(), "channels must have identical length");
        let stereo = left
            .into_iter()
            .zip(right.into_iter())
            .map(|(l, r)| [l, r])
            .collect();
        AudioChunkData::new(stereo, sample_rate, 0.0)
    }
}

// Méthodes spécifiques pour f64
impl AudioChunkData<f64> {
    /// Applique le gain et retourne un nouveau chunk avec les données modifiées
    pub fn apply_gain(self: Arc<Self>) -> Arc<Self> {
        if self.gain_db.abs() < f64::EPSILON {
            return self; // Pas de gain à appliquer
        }

        let gain_linear = db_to_linear(self.gain_db);
        let mut stereo = self.clone_frames();
        for frame in &mut stereo {
            frame[0] *= gain_linear;
            frame[1] *= gain_linear;
        }

        AudioChunkData::new(stereo, self.sample_rate, 0.0)
    }

    /// Construit un chunk depuis deux vecteurs `f64` séparés (L/R)
    pub fn from_channels(left: Vec<f64>, right: Vec<f64>, sample_rate: u32) -> Arc<Self> {
        assert_eq!(left.len(), right.len(), "channels must have identical length");
        let stereo = left
            .into_iter()
            .zip(right.into_iter())
            .map(|(l, r)| [l, r])
            .collect();
        AudioChunkData::new(stereo, sample_rate, 0.0)
    }
}

// ============================================================================
// AudioChunk : Enum pour tous les types de chunks
// ============================================================================

/// Enum contenant tous les types de chunks audio possibles
///
/// Cette enum permet de manipuler des chunks de différents types dans un
/// pipeline unifié, tout en conservant l'information de type.
///
/// # Variantes
///
/// - `I8` : Échantillons 8-bit signés
/// - `I16` : Échantillons 16-bit signés
/// - `I24` : Échantillons 24-bit signés (stockés sur i32)
/// - `I32` : Échantillons 32-bit signés
/// - `F32` : Échantillons flottants 32-bit normalisés [-1.0, 1.0]
/// - `F64` : Échantillons flottants 64-bit normalisés [-1.0, 1.0]
///
/// # Exemples
///
/// ```
/// use pmoaudio::{AudioChunk, AudioChunkData};
///
/// let chunk_f32 = AudioChunkData::new(vec![[0.5f32, 0.25f32]; 1000], 48_000, 0.0);
/// let chunk = AudioChunk::F32(chunk_f32);
///
/// match &chunk {
///     AudioChunk::F32(data) => println!("F32 chunk with {} frames", data.len()),
///     _ => println!("Other type"),
/// }
/// ```
#[derive(Debug, Clone)]
pub enum AudioChunk {
    I8(Arc<AudioChunkData<i8>>),
    I16(Arc<AudioChunkData<i16>>),
    I24(Arc<AudioChunkData<I24>>),
    I32(Arc<AudioChunkData<i32>>),
    F32(Arc<AudioChunkData<f32>>),
    F64(Arc<AudioChunkData<f64>>),
}

impl AudioChunk {
    /// Retourne le nombre de frames du chunk
    pub fn len(&self) -> usize {
        match self {
            AudioChunk::I8(d) => d.len(),
            AudioChunk::I16(d) => d.len(),
            AudioChunk::I24(d) => d.len(),
            AudioChunk::I32(d) => d.len(),
            AudioChunk::F32(d) => d.len(),
            AudioChunk::F64(d) => d.len(),
        }
    }

    /// Vérifie si le chunk est vide
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Taux d'échantillonnage (Hz)
    pub fn sample_rate(&self) -> u32 {
        match self {
            AudioChunk::I8(d) => d.sample_rate(),
            AudioChunk::I16(d) => d.sample_rate(),
            AudioChunk::I24(d) => d.sample_rate(),
            AudioChunk::I32(d) => d.sample_rate(),
            AudioChunk::F32(d) => d.sample_rate(),
            AudioChunk::F64(d) => d.sample_rate(),
        }
    }

    /// Gain courant en décibels
    pub fn gain_db(&self) -> f64 {
        match self {
            AudioChunk::I8(d) => d.gain_db(),
            AudioChunk::I16(d) => d.gain_db(),
            AudioChunk::I24(d) => d.gain_db(),
            AudioChunk::I32(d) => d.gain_db(),
            AudioChunk::F32(d) => d.gain_db(),
            AudioChunk::F64(d) => d.gain_db(),
        }
    }

    /// Gain sous forme linéaire
    pub fn gain_linear(&self) -> f64 {
        db_to_linear(self.gain_db())
    }

    /// Définit le gain en dB
    pub fn set_gain_db(&self, gain_db: f64) -> Self {
        match self {
            AudioChunk::I8(d) => AudioChunk::I8(d.set_gain_db(gain_db)),
            AudioChunk::I16(d) => AudioChunk::I16(d.set_gain_db(gain_db)),
            AudioChunk::I24(d) => AudioChunk::I24(d.set_gain_db(gain_db)),
            AudioChunk::I32(d) => AudioChunk::I32(d.set_gain_db(gain_db)),
            AudioChunk::F32(d) => AudioChunk::F32(d.set_gain_db(gain_db)),
            AudioChunk::F64(d) => AudioChunk::F64(d.set_gain_db(gain_db)),
        }
    }

    /// Définit le gain via un facteur linéaire
    pub fn set_gain_linear(&self, gain_linear: f64) -> Self {
        self.set_gain_db(linear_to_db(gain_linear))
    }

    /// Modifie le gain (ajoute un delta en dB)
    pub fn with_modified_gain_db(&self, delta_gain_db: f64) -> Self {
        self.set_gain_db(self.gain_db() + delta_gain_db)
    }

    /// Applique le gain et retourne un nouveau chunk avec les données modifiées
    ///
    /// Le gain du chunk résultant est remis à 0.0 dB.
    pub fn apply_gain(self) -> Self {
        match self {
            AudioChunk::I8(d) => {
                // Pour i8, on convert en i32, applique gain, puis reconvertit
                // TODO: optimiser avec une version directe
                let gain_db = d.gain_db();
                if gain_db.abs() < f64::EPSILON {
                    return AudioChunk::I8(d);
                }
                let gain_linear = db_to_linear(gain_db) as f32;
                let mut stereo = d.clone_frames();
                for frame in &mut stereo {
                    frame[0] = (frame[0] as f32 * gain_linear).round().clamp(-128.0, 127.0) as i8;
                    frame[1] = (frame[1] as f32 * gain_linear).round().clamp(-128.0, 127.0) as i8;
                }
                AudioChunk::I8(AudioChunkData::new(stereo, d.sample_rate(), 0.0))
            }
            AudioChunk::I16(d) => {
                let gain_db = d.gain_db();
                if gain_db.abs() < f64::EPSILON {
                    return AudioChunk::I16(d);
                }
                let gain_linear = db_to_linear(gain_db) as f32;
                let mut stereo = d.clone_frames();
                for frame in &mut stereo {
                    frame[0] = (frame[0] as f32 * gain_linear).round().clamp(-32768.0, 32767.0) as i16;
                    frame[1] = (frame[1] as f32 * gain_linear).round().clamp(-32768.0, 32767.0) as i16;
                }
                AudioChunk::I16(AudioChunkData::new(stereo, d.sample_rate(), 0.0))
            }
            AudioChunk::I24(d) => {
                let gain_db = d.gain_db();
                if gain_db.abs() < f64::EPSILON {
                    return AudioChunk::I24(d);
                }
                let gain_linear = db_to_linear(gain_db) as f32;
                let mut stereo = d.clone_frames();
                for frame in &mut stereo {
                    let l = (frame[0].as_i32() as f32 * gain_linear).round().clamp(-8_388_608.0, 8_388_607.0) as i32;
                    let r = (frame[1].as_i32() as f32 * gain_linear).round().clamp(-8_388_608.0, 8_388_607.0) as i32;
                    frame[0] = I24::new_clamped(l);
                    frame[1] = I24::new_clamped(r);
                }
                AudioChunk::I24(AudioChunkData::new(stereo, d.sample_rate(), 0.0))
            }
            AudioChunk::I32(d) => AudioChunk::I32(d.apply_gain()),
            AudioChunk::F32(d) => AudioChunk::F32(d.apply_gain()),
            AudioChunk::F64(d) => AudioChunk::F64(d.apply_gain()),
        }
    }

    /// Retourne le nom du type de sample
    pub fn type_name(&self) -> &'static str {
        match self {
            AudioChunk::I8(_) => "i8",
            AudioChunk::I16(_) => "i16",
            AudioChunk::I24(_) => "I24",
            AudioChunk::I32(_) => "i32",
            AudioChunk::F32(_) => "f32",
            AudioChunk::F64(_) => "f64",
        }
    }
}

// ============================================================================
// Fonctions utilitaires de conversion gain
// ============================================================================

const MIN_GAIN_DB: f64 = -120.0;

/// Convertit un gain linéaire (>0) en décibels
#[inline]
pub fn linear_to_db(gain_linear: f64) -> f64 {
    if gain_linear <= 0.0 {
        MIN_GAIN_DB
    } else {
        (20.0 * gain_linear.log10()).max(MIN_GAIN_DB)
    }
}

/// Convertit un gain en décibels vers un gain linéaire
#[inline]
pub fn db_to_linear(gain_db: f64) -> f64 {
    10f64.powf(gain_db / 20.0)
}

/// Convertit un gain linéaire en décibels (méthode publique pour compatibilité)
pub fn gain_db_from_linear(gain_linear: f64) -> f64 {
    linear_to_db(gain_linear)
}

/// Convertit un gain en décibels vers un gain linéaire (méthode publique pour compatibilité)
pub fn gain_linear_from_db(gain_db: f64) -> f64 {
    db_to_linear(gain_db)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_chunk_data_f32() {
        let stereo: Vec<[f32; 2]> = vec![[0.5, 0.25], [0.75, 0.125]];
        let chunk = AudioChunkData::new(stereo, 48000, 0.0);

        assert_eq!(chunk.len(), 2);
        assert_eq!(chunk.sample_rate(), 48000);
        assert!(!chunk.is_empty());
        assert_eq!(chunk.gain_db(), 0.0);
    }

    #[test]
    fn test_audio_chunk_data_i32() {
        let stereo: Vec<[i32; 2]> = vec![[1000, 2000], [3000, 4000]];
        let chunk = AudioChunkData::new(stereo, 48000, -6.0);

        assert_eq!(chunk.len(), 2);
        assert_eq!(chunk.gain_db(), -6.0);
    }

    #[test]
    fn test_audio_chunk_enum() {
        let data_f32 = AudioChunkData::new(vec![[0.5f32, 0.25f32]; 1000], 48000, 0.0);
        let chunk = AudioChunk::F32(data_f32);

        assert_eq!(chunk.len(), 1000);
        assert_eq!(chunk.sample_rate(), 48000);
        assert_eq!(chunk.type_name(), "f32");
    }

    #[test]
    fn test_gain_conversion() {
        let linear = 2.0;
        let db = linear_to_db(linear);
        assert!((db - 6.0206).abs() < 0.01); // 2x ≈ +6dB

        let back = db_to_linear(db);
        assert!((back - linear).abs() < 0.001);
    }
}
