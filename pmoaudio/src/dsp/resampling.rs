use soxr::format::Stereo;
use soxr::params::{QualityRecipe, QualitySpec, RuntimeSpec};
use soxr::Soxr;

use crate::dsp::{i32_stereo_to_pairs_f32, pairs_f32_to_i32_stereo};
use crate::BitDepth;

// Type d'erreur simple pour resampling
#[derive(Debug)]
pub struct ResamplingError(pub String);

impl std::fmt::Display for ResamplingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Resampling error: {}", self.0)
    }
}

impl std::error::Error for ResamplingError {}

pub struct Resampler {
    source_hz: f64,
    dest_hz: f64,
    bit_depth: BitDepth,
    soxr: Soxr<Stereo<f32>>,
}

pub fn build_resampler(
    source_hz: u32,
    dest_hz: u32,
    bit_depth: BitDepth,
) -> Result<Resampler, ResamplingError> {
    let qrecipe = match bit_depth {
        BitDepth::B8 => QualityRecipe::Medium,
        BitDepth::B16 => QualityRecipe::high(), // High pour 16-bit
        BitDepth::B24 => QualityRecipe::very_high(), // VeryHigh pour 24-bit
        BitDepth::B32 => QualityRecipe::very_high(), // VeryHigh pour 32-bit
    };

    let quality = QualitySpec::new(qrecipe); // Phase response linear, no steep filter
    let rt = RuntimeSpec::default();

    let soxr = Soxr::<Stereo<f32>>::new_with_params(source_hz as f64, dest_hz as f64, quality, rt)
        .map_err(|e| ResamplingError(e.to_string()))?;

    Ok(Resampler {
        source_hz: source_hz as f64,
        dest_hz: dest_hz as f64,
        bit_depth,
        soxr,
    })
}

pub fn resampling(left: &[i32], right: &[i32], resampler: &mut Resampler) -> (Vec<i32>, Vec<i32>) {
    if left.len() != right.len() {
        panic!("Left and right channels must have the same length");
    }

    // Convertir i32 → f32 normalisé
    let mut input = vec![[0.0f32; 2]; left.len()];
    i32_stereo_to_pairs_f32(left, right, &mut input, resampler.bit_depth);

    // Resampling
    let output_len =
        ((input.len() as f64) * resampler.dest_hz / resampler.source_hz).ceil() as usize;
    let mut output = vec![[0.0f32; 2]; output_len];

    resampler.soxr.process(&input, &mut output).unwrap();

    // Convertir f32 normalisé → i32
    let mut oleft = vec![0i32; output.len()];
    let mut oright = vec![0i32; output.len()];
    pairs_f32_to_i32_stereo(&output, &mut oleft, &mut oright, resampler.bit_depth);

    (oleft, oright)
}
