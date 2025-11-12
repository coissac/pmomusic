//! Module DSP pour les conversions et traitements audio optimisés (SIMD)
//!
//! Ce module fournit des fonctions optimisées pour le traitement audio numérique (DSP).
//! Toutes les fonctions sont optimisées avec SIMD lorsque la feature "simd" est activée.
//!
//! # Sous-modules
//!
//! - [`depth`] - Conversion de profondeur de bit (bit-depth) entre 16/24/32 bits
//! - [`gain_16bits`] - Application de gain pour audio 16-bit
//! - [`gain_24bits`] - Application de gain pour audio 24-bit
//! - [`gain_32bits`] - Application de gain pour audio 32-bit
//! - [`int_float`] - Conversions entre formats entiers et virgule flottante
//! - [`resampling`] - Rééchantillonnage (wrapper autour de libsoxr)
//!
//! # Optimisations
//!
//! Les fonctions de ce module utilisent des optimisations SIMD (Single Instruction Multiple Data)
//! pour traiter plusieurs échantillons en parallèle quand la feature "simd" est activée.
//! Sans SIMD, les implémentations scalaires restent efficaces grâce aux optimisations du compilateur.
//!
//! # Exemples
//!
//! ```
//! use pmoaudio::dsp::{bitdepth_change_stereo, apply_gain_stereo_i32};
//! use pmoaudio::BitDepth;
//!
//! // Conversion de bit-depth
//! let mut data = vec![[1000i32, 2000i32]];
//! bitdepth_change_stereo(&mut data, BitDepth::B16, BitDepth::B24);
//!
//! // Application de gain
//! let mut data = vec![[100000i32, 200000i32]];
//! apply_gain_stereo_i32(&mut data, 2.0); // +6dB
//! ```

pub mod depth;
pub mod gain_16bits;
pub mod gain_24bits;
pub mod gain_32bits;
pub mod int_float;
pub mod resampling;

pub use depth::bitdepth_change_stereo;
pub use gain_16bits::apply_gain_stereo_i16;
pub use gain_24bits::apply_gain_stereo_i24;
pub use gain_32bits::apply_gain_stereo_i32;

pub use int_float::{
    i16_stereo_to_pairs_f32, i24_as_i32_stereo_to_pairs_f32, i32_stereo_to_interleaved_f32,
    i32_stereo_to_pairs_f32, interleaved_f32_to_i32_stereo, pairs_f32_to_i16_stereo,
    pairs_f32_to_i24_as_i32_stereo, pairs_f32_to_i32_stereo,
};

pub use resampling::resampling;
