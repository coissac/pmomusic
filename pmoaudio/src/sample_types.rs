//! Types de samples audio et trait de conversion générique

use std::fmt;

/// Trait pour tous les types de samples audio supportés
///
/// Ce trait permet d'écrire du code générique sur différents types de samples
/// (entiers 8/16/24/32 bits et flottants 32/64 bits).
pub trait Sample: Copy + Clone + Send + Sync + 'static + fmt::Debug {
    /// Nom du type pour le débogage
    const NAME: &'static str;

    /// Valeur minimale du type
    const MIN: Self;

    /// Valeur maximale du type
    const MAX: Self;

    /// Valeur zéro
    const ZERO: Self;

    /// Convertit le sample en f64 normalisé dans [-1.0, 1.0]
    fn to_f64(self) -> f64;

    /// Crée un sample depuis un f64 normalisé dans [-1.0, 1.0]
    fn from_f64(value: f64) -> Self;

    /// Convertit le sample en f32 normalisé dans [-1.0, 1.0]
    fn to_f32(self) -> f32 {
        self.to_f64() as f32
    }

    /// Crée un sample depuis un f32 normalisé dans [-1.0, 1.0]
    fn from_f32(value: f32) -> Self {
        Self::from_f64(value as f64)
    }
}

// ============================================================================
// Type I24 : Échantillon audio 24-bit stocké dans un i32
// ============================================================================

/// Échantillon audio 24-bit signé, stocké dans un i32
///
/// Représente un sample audio de 24 bits de résolution effective,
/// stocké sur 32 bits pour l'alignement et les performances.
///
/// Plage valide : [-8_388_608, 8_388_607] (±2^23)
///
/// # Exemples
///
/// ```
/// use pmoaudio::I24;
///
/// let sample = I24::new(1_000_000).unwrap();
/// assert_eq!(sample.as_i32(), 1_000_000);
///
/// // Hors plage : erreur
/// assert!(I24::new(10_000_000).is_none());
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct I24(i32);

impl I24 {
    /// Valeur minimale : -2^23
    pub const MIN_VALUE: i32 = -8_388_608;

    /// Valeur maximale : 2^23 - 1
    pub const MAX_VALUE: i32 = 8_388_607;

    /// Valeur zéro
    pub const ZERO: I24 = I24(0);

    /// Valeur minimale
    pub const MIN: I24 = I24(Self::MIN_VALUE);

    /// Valeur maximale
    pub const MAX: I24 = I24(Self::MAX_VALUE);

    /// Crée un nouveau I24 depuis un i32, en vérifiant la plage valide
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoaudio::I24;
    ///
    /// assert!(I24::new(0).is_some());
    /// assert!(I24::new(8_388_607).is_some());
    /// assert!(I24::new(-8_388_608).is_some());
    /// assert!(I24::new(10_000_000).is_none()); // Hors plage
    /// ```
    #[inline]
    pub const fn new(value: i32) -> Option<Self> {
        if value >= Self::MIN_VALUE && value <= Self::MAX_VALUE {
            Some(I24(value))
        } else {
            None
        }
    }

    /// Crée un nouveau I24 depuis un i32, en clampant à la plage valide
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoaudio::I24;
    ///
    /// assert_eq!(I24::new_clamped(10_000_000).as_i32(), 8_388_607);
    /// assert_eq!(I24::new_clamped(-10_000_000).as_i32(), -8_388_608);
    /// ```
    #[inline]
    pub const fn new_clamped(value: i32) -> Self {
        let clamped = if value < Self::MIN_VALUE {
            Self::MIN_VALUE
        } else if value > Self::MAX_VALUE {
            Self::MAX_VALUE
        } else {
            value
        };
        I24(clamped)
    }

    /// Crée un nouveau I24 depuis un i32 sans vérification
    ///
    /// # Safety
    ///
    /// Le caller doit garantir que `value` est dans [-8_388_608, 8_388_607]
    #[inline]
    pub const unsafe fn new_unchecked(value: i32) -> Self {
        I24(value)
    }

    /// Retourne la valeur i32 interne
    #[inline]
    pub const fn as_i32(self) -> i32 {
        self.0
    }

    /// Retourne la valeur i32 interne (alias pour compatibilité)
    #[inline]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl fmt::Debug for I24 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "I24({})", self.0)
    }
}

impl fmt::Display for I24 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<I24> for i32 {
    #[inline]
    fn from(i24: I24) -> i32 {
        i24.0
    }
}

impl TryFrom<i32> for I24 {
    type Error = &'static str;

    #[inline]
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        I24::new(value).ok_or("i32 value out of I24 range")
    }
}

// ============================================================================
// Implémentations du trait Sample pour tous les types
// ============================================================================

impl Sample for i8 {
    const NAME: &'static str = "i8";
    const MIN: Self = i8::MIN;
    const MAX: Self = i8::MAX;
    const ZERO: Self = 0;

    #[inline]
    fn to_f64(self) -> f64 {
        self as f64 / 128.0
    }

    #[inline]
    fn from_f64(value: f64) -> Self {
        (value * 127.0).clamp(-128.0, 127.0).round() as i8
    }
}

impl Sample for i16 {
    const NAME: &'static str = "i16";
    const MIN: Self = i16::MIN;
    const MAX: Self = i16::MAX;
    const ZERO: Self = 0;

    #[inline]
    fn to_f64(self) -> f64 {
        self as f64 / 32_768.0
    }

    #[inline]
    fn from_f64(value: f64) -> Self {
        (value * 32_767.0).clamp(-32_768.0, 32_767.0).round() as i16
    }
}

impl Sample for I24 {
    const NAME: &'static str = "I24";
    const MIN: Self = I24::MIN;
    const MAX: Self = I24::MAX;
    const ZERO: Self = I24::ZERO;

    #[inline]
    fn to_f64(self) -> f64 {
        self.0 as f64 / 8_388_608.0
    }

    #[inline]
    fn from_f64(value: f64) -> Self {
        let scaled = (value * 8_388_607.0).clamp(-8_388_608.0, 8_388_607.0).round() as i32;
        I24(scaled)
    }
}

impl Sample for i32 {
    const NAME: &'static str = "i32";
    const MIN: Self = i32::MIN;
    const MAX: Self = i32::MAX;
    const ZERO: Self = 0;

    #[inline]
    fn to_f64(self) -> f64 {
        self as f64 / 2_147_483_648.0
    }

    #[inline]
    fn from_f64(value: f64) -> Self {
        (value * 2_147_483_647.0).clamp(-2_147_483_648.0, 2_147_483_647.0).round() as i32
    }
}

impl Sample for f32 {
    const NAME: &'static str = "f32";
    const MIN: Self = -1.0;
    const MAX: Self = 1.0;
    const ZERO: Self = 0.0;

    #[inline]
    fn to_f64(self) -> f64 {
        self as f64
    }

    #[inline]
    fn from_f64(value: f64) -> Self {
        value as f32
    }

    #[inline]
    fn to_f32(self) -> f32 {
        self
    }

    #[inline]
    fn from_f32(value: f32) -> Self {
        value
    }
}

impl Sample for f64 {
    const NAME: &'static str = "f64";
    const MIN: Self = -1.0;
    const MAX: Self = 1.0;
    const ZERO: Self = 0.0;

    #[inline]
    fn to_f64(self) -> f64 {
        self
    }

    #[inline]
    fn from_f64(value: f64) -> Self {
        value
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i24_creation() {
        assert_eq!(I24::new(0).unwrap().as_i32(), 0);
        assert_eq!(I24::new(8_388_607).unwrap().as_i32(), 8_388_607);
        assert_eq!(I24::new(-8_388_608).unwrap().as_i32(), -8_388_608);

        assert!(I24::new(8_388_608).is_none());
        assert!(I24::new(-8_388_609).is_none());
        assert!(I24::new(10_000_000).is_none());
    }

    #[test]
    fn test_i24_clamped() {
        assert_eq!(I24::new_clamped(10_000_000).as_i32(), 8_388_607);
        assert_eq!(I24::new_clamped(-10_000_000).as_i32(), -8_388_608);
        assert_eq!(I24::new_clamped(1_000_000).as_i32(), 1_000_000);
    }

    #[test]
    fn test_sample_trait_i24() {
        let sample = I24::new(4_194_303).unwrap(); // ~0.5 en normalized
        let normalized = sample.to_f64();
        assert!((normalized - 0.5).abs() < 0.001);

        let back = I24::from_f64(0.5);
        assert!((back.as_i32() - 4_194_303).abs() <= 1); // Tolérance d'arrondi
    }

    #[test]
    fn test_sample_trait_roundtrip_i16() {
        let original: i16 = 16_000;
        let normalized = original.to_f64();
        let back = i16::from_f64(normalized);
        assert!((back - original).abs() <= 1);
    }

    #[test]
    fn test_sample_trait_roundtrip_f32() {
        let original: f32 = 0.75;
        let normalized = original.to_f64();
        let back = f32::from_f64(normalized);
        assert!((back - original).abs() < 1e-6);
    }
}
