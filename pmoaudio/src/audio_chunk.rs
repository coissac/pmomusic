use std::sync::Arc;

use crate::{dsp, BitDepth};

/// Représente un chunk audio stéréo avec données partagées via Arc
///
/// Cette structure encapsule des données audio stéréo (canaux gauche et droit)
/// en utilisant `Arc<Vec<f32>>` pour permettre le partage efficace entre plusieurs
/// consumers sans copier les données audio.
///
/// # Optimisation zero-copy
///
/// Les données audio sont wrappées dans `Arc`, ce qui signifie que:
/// - Le clonage d'un `AudioChunk` ne clone que les pointeurs Arc (très rapide)
/// - Les données audio réelles ne sont copiées que si nécessaire (Copy-on-Write)
/// - Plusieurs nodes peuvent partager le même chunk simultanément
///
/// # Exemples
///
/// ```
/// use pmoaudio::{AudioChunk, BitDepth};
///
/// // Créer un chunk avec des données générées
/// let stereo = vec![[0, 100], [200, 300], [400, 500]];
/// let chunk = AudioChunk::new(0, stereo, 48_000, BitDepth::B24);
///
/// assert_eq!(chunk.len(), 3);
/// assert_eq!(chunk.sample_rate(), 48_000);
/// ```

#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// Numéro d’ordre dans le flux.  
    /// Sert à conserver la séquence et détecter d’éventuelles pertes.
    order: u64,

    /// Canal gauche, partagé et immuable.  
    /// Toute transformation doit créer un nouveau `AudioChunk`.
    stereo: Arc<[[i32; 2]]>,

    /// Taux d’échantillonnage (Hz).  
    /// Exemples : 44 100, 48 000, 96 000, 192 000.
    sample_rate: u32,

    /// Profondeur de bits des échantillons audio effectifs.  
    ///
    /// Indique la résolution utile des valeurs dans les buffers.  
    /// Exemples : `16` pour un flux PCM 16 bits, `24` pour du PCM 24 bits, `32` pour du plein i32.  
    /// Ce champ permet d’adapter les traitements DSP (normalisation, conversion, etc.).
    bit_depth: BitDepth,

    /// Gain appliqué au flux audio, en décibels (dB).  
    ///
    /// Conversion : `gain_linear = 10^(gain_db / 20)`  
    /// Valeur par défaut : `0.0 dB` (aucune modification).  
    /// Exemples : `-6 dB` ≈ moitié du volume ; `+6 dB` ≈ double.
    gain: f64,
}

impl AudioChunk {
    /// Crée un nouveau chunk audio
    ///
    /// Les vecteurs sont automatiquement wrappés dans `Arc`.
    ///
    /// # Arguments
    ///
    /// * `order` - Numéro d'ordre du chunk dans le flux
    /// * `stereo` - Samples interleavés par frame `[L, R]`
    /// * `sample_rate` - Taux d'échantillonnage en Hz
    /// * `bit_depth` - Profondeur de bits des échantillons
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoaudio::{AudioChunk, BitDepth};
    ///
    /// let chunk = AudioChunk::new(
    ///     0,
    ///     vec![[0, 0], [1_000_000, 1_000_000]],
    ///     48_000,
    ///     BitDepth::B24,
    /// );
    /// ```
    pub fn new(
        order: u64,
        stereo: Vec<[i32; 2]>,
        sample_rate: u32,
        bit_depth: BitDepth,
    ) -> Arc<Self> {
        Arc::new(Self {
            order,
            stereo: Arc::from(stereo),
            sample_rate,
            bit_depth,
            gain: 0.0,
        })
    }

    /// Crée un chunk avec un gain spécifique (en dB)
    pub fn with_gain_db(
        order: u64,
        stereo: Vec<[i32; 2]>,
        sample_rate: u32,
        bit_depth: BitDepth,
        gain_db: f64,
    ) -> Arc<Self> {
        Self::new(order, stereo, sample_rate, bit_depth).set_gain_db(gain_db)
    }

    /// Crée un chunk avec un gain spécifique (en gain linéaire).
    ///
    /// Le gain linéaire sera converti en décibels.
    pub fn with_gain_linear(
        order: u64,
        stereo: Vec<[i32; 2]>,
        sample_rate: u32,
        bit_depth: BitDepth,
        gain_linear: f64,
    ) -> Arc<Self> {
        Self::new(order, stereo, sample_rate, bit_depth).set_gain_linear(gain_linear)
    }

    /// Construit un chunk à partir de deux vecteurs `i32` séparés (L/R).
    pub fn from_channels_i32(
        order: u64,
        left: Vec<i32>,
        right: Vec<i32>,
        sample_rate: u32,
        bit_depth: BitDepth,
    ) -> Arc<Self> {
        assert_eq!(
            left.len(),
            right.len(),
            "channels must have identical length"
        );
        let stereo = left
            .into_iter()
            .zip(right.into_iter())
            .map(|(l, r)| [l, r])
            .collect();
        Self::new(order, stereo, sample_rate, bit_depth)
    }

    /// Construit un chunk à partir de vecteurs `f32` normalisés dans [-1.0, 1.0].
    pub fn from_channels_f32(
        order: u64,
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
        let stereo = left
            .into_iter()
            .zip(right.into_iter())
            .map(|(l, r)| [quantize_sample(l, bit_depth), quantize_sample(r, bit_depth)])
            .collect();
        Self::new(order, stereo, sample_rate, bit_depth)
    }

    /// Construit un chunk à partir de frames stéréo normalisées [-1.0, 1.0].
    pub fn from_pairs_f32(
        order: u64,
        pairs: Vec<[f32; 2]>,
        sample_rate: u32,
        bit_depth: BitDepth,
    ) -> Arc<Self> {
        let stereo = pairs
            .into_iter()
            .map(|p| {
                [
                    quantize_sample(p[0], bit_depth),
                    quantize_sample(p[1], bit_depth),
                ]
            })
            .collect();
        Self::new(order, stereo, sample_rate, bit_depth)
    }

    /// Retourne le nombre d'échantillons par canal
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoaudio::{AudioChunk, BitDepth};
    ///
    /// let chunk = AudioChunk::new(0, vec![[0i32; 2]; 1000], 48_000, BitDepth::B24);
    /// assert_eq!(chunk.len(), 1000);
    /// ```
    pub fn len(&self) -> usize {
        self.stereo.len()
    }

    /// Vérifie si le chunk est vide
    pub fn is_empty(&self) -> bool {
        self.stereo.is_empty()
    }

    /// Numéro de séquence du chunk dans le flux.
    pub fn order(&self) -> u64 {
        self.order
    }

    /// Taux d'échantillonnage (Hz).
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Profondeur de bits effective.
    pub fn bit_depth(&self) -> BitDepth {
        self.bit_depth
    }

    /// Gain courant en décibels.
    pub fn gain_db(&self) -> f64 {
        self.gain
    }

    /// Gain sous forme linéaire.
    pub fn gain_linear(&self) -> f64 {
        db_to_linear(self.gain)
    }

    /// Convertit un gain linéaire (>0) en décibels.
    pub fn gain_db_from_linear(gain_linear: f64) -> f64 {
        linear_to_db(gain_linear)
    }

    /// Convertit un gain en décibels vers un gain linéaire.
    pub fn gain_linear_from_db(gain_db: f64) -> f64 {
        db_to_linear(gain_db)
    }

    /// Retourne une vue immuable sur les frames `[L,R]`.
    pub fn frames(&self) -> &[[i32; 2]] {
        &self.stereo
    }

    /// Clone les frames stéréo dans un `Vec`.
    pub fn clone_frames(&self) -> Vec<[i32; 2]> {
        self.stereo.to_vec()
    }

    /// Convertit les frames au format `f32` normalisé [-1.0, 1.0].
    pub fn to_pairs_f32(&self) -> Vec<[f32; 2]> {
        self.stereo
            .iter()
            .map(|frame| {
                [
                    dequantize_sample(frame[0], self.bit_depth),
                    dequantize_sample(frame[1], self.bit_depth),
                ]
            })
            .collect()
    }

    /// Clone les données pour permettre une modification (Copy-on-Write)
    ///
    /// Cette méthode doit être appelée uniquement si vous avez besoin de modifier
    /// les échantillons. Pour une simple lecture, utilisez [`frames`](Self::frames).
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoaudio::{AudioChunk, BitDepth};
    ///
    /// let chunk = AudioChunk::new(0, vec![[1, 2], [3, 4]], 48_000, BitDepth::B24);
    /// let mut frames = chunk.clone_data();
    /// frames[0][0] /= 2;
    /// ```
    pub fn clone_data(&self) -> Vec<[i32; 2]> {
        self.stereo.to_vec()
    }

    pub fn set_data(&mut self, stereo: Vec<[i32; 2]>) {
        self.stereo = Arc::from(stereo);
    }

    /// Applique le gain et retourne un nouveau chunk avec les données modifiées
    ///
    /// Cette méthode crée un nouveau chunk avec les samples multipliés par le gain.
    /// Utile pour les nodes qui doivent matérialiser le gain avant la sortie.
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoaudio::{AudioChunk, BitDepth};
    ///
    /// let chunk = AudioChunk::from_pairs_f32(
    ///     0,
    ///     vec![[0.5, 0.25], [0.25, 0.125]],
    ///     48_000,
    ///     BitDepth::B24,
    /// );
    /// let chunk = chunk.set_gain_linear(0.5);
    /// let applied = chunk.apply_gain();
    /// let frames = applied.to_pairs_f32();
    ///
    /// assert!((frames[0][0] - 0.25).abs() < 1e-3);
    /// assert!((applied.gain_db()).abs() < f64::EPSILON); // Gain réinitialisé après application
    /// ```
    pub fn apply_gain(self: Arc<Self>) -> Arc<Self> {
        if self.gain.abs() < f64::EPSILON {
            // Pas de gain à appliquer, retourner la même instance
            return self;
        }

        let mut stereo = self.clone_data();
        dsp::apply_gain_stereo(&mut stereo, self.gain);

        Self::new(self.order, stereo, self.sample_rate, self.bit_depth)
    }

    pub fn set_gain_db(&self, gain: f64) -> Arc<Self> {
        Arc::new(Self {
            order: self.order,
            stereo: self.stereo.clone(),
            sample_rate: self.sample_rate,
            bit_depth: self.bit_depth,
            gain,
        })
    }

    /// Définit le gain à l'aide d'un facteur linéaire (>0).
    pub fn set_gain_linear(&self, gain_linear: f64) -> Arc<Self> {
        self.set_gain_db(linear_to_db(gain_linear))
    }

    /// Modifie le gain de ce chunk (retourne un nouveau chunk avec le même Arc mais gain différent)
    ///
    /// Cette méthode est très peu coûteuse car elle ne clone que la structure, pas les données audio.
    pub fn with_modified_gain_db(&self, delta_gain_db: f64) -> Arc<Self> {
        self.set_gain_db(self.gain + delta_gain_db)
    }

    /// Modifie le gain via un facteur linéaire multiplié au gain courant.
    pub fn with_modified_gain_linear(&self, gain_linear: f64) -> Arc<Self> {
        self.with_modified_gain_db(linear_to_db(gain_linear))
    }

    pub fn get_bit_depth(&self) -> BitDepth {
        self.bit_depth
    }
    pub fn set_bit_depth(self: Arc<Self>, new_depth: BitDepth) -> Arc<Self> {
        if self.bit_depth == new_depth {
            return self;
        }

        let mut stereo = self.clone_data();
        dsp::bitdepth_change_stereo(&mut stereo, self.bit_depth, new_depth);

        Arc::new(Self {
            order: self.order,
            stereo: Arc::from(stereo),
            sample_rate: self.sample_rate,
            bit_depth: new_depth,
            gain: self.gain,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_chunk_creation() {
        let stereo: Vec<[i32; 2]> = vec![
            [0, 10],  // frame 0 : L=0, R=10
            [20, 30], // frame 1 : L=20, R=30
            [40, 50], // frame 2 : L=40, R=50
        ];
        let chunk = AudioChunk::new(0, stereo, 48000, BitDepth::B24);

        assert_eq!(chunk.order(), 0);
        assert_eq!(chunk.len(), 3);
        assert_eq!(chunk.sample_rate(), 48000);
        assert!(!chunk.is_empty());
    }
}

fn quantize_sample(sample: f32, bit_depth: BitDepth) -> i32 {
    let max_value = bit_depth.max_value() as f64;
    let upper = max_value - 1.0;
    let lower = -max_value;
    let scaled = (sample as f64 * upper).round();
    scaled.clamp(lower, upper) as i32
}

fn dequantize_sample(sample: i32, bit_depth: BitDepth) -> f32 {
    let max_value = bit_depth.max_value();
    sample as f32 / max_value
}

const MIN_GAIN_DB: f64 = -120.0;

fn linear_to_db(gain_linear: f64) -> f64 {
    if gain_linear <= 0.0 {
        MIN_GAIN_DB
    } else {
        (20.0 * gain_linear.log10()).max(MIN_GAIN_DB)
    }
}

fn db_to_linear(gain_db: f64) -> f64 {
    10f64.powf(gain_db / 20.0)
}
