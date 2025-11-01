//! Module DSP pour les conversions et traitements audio optimisés (SIMD)

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
