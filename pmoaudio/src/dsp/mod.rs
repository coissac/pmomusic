//! Module DSP pour les conversions et traitements audio optimisés (SIMD)

pub mod depth;
pub mod gain;
pub mod int_float;
pub mod resampling;

pub use depth::bitdepth_change_stereo;
pub use gain::apply_gain_stereo;
pub use int_float::{
    i32_stereo_to_interleaved_f32, i32_stereo_to_pairs_f32, interleaved_f32_to_i32_stereo,
    pairs_f32_to_i32_stereo,
};

pub use resampling::resampling;
