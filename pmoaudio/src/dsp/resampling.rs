use soxr::format::Stereo;
use soxr::params::{QualityRecipe, QualitySpec, RuntimeSpec};
use soxr::Soxr;

use crate::dsp::int_float::{Bit16, Bit24, Bit32, Bit8};
use crate::dsp::{i32_stereo_to_pairs_f32, pairs_f32_to_i32_stereo};
use crate::AudioError;

pub struct Resampler {
    source_hz: f64,
    dest_hz: f64,
    bit_depth: u32,
    soxr: Soxr<Stereo<f32>>,
}

pub fn build_resampler(
    source_hz: u32,
    dest_hz: u32,
    bit_depth: u32,
) -> Result<Resampler, AudioError> {
    let qrecipe = match bit_depth {
        8 => QualityRecipe::Medium,
        16 => QualityRecipe::high(), // High plutôt que Bits16 pour 16-bit
        24 => QualityRecipe::very_high(), // VeryHigh pour 24-bit
        32 => QualityRecipe::very_high(), // VeryHigh pour 32-bit
        _ => unreachable!(),         // Déjà vérifié plus haut
    };

    let quality = QualitySpec::new(qrecipe); // Phase response linear, no steep filter
    let rt = RuntimeSpec::default();

    let soxr = Soxr::<Stereo<f32>>::new_with_params(source_hz as f64, dest_hz as f64, quality, rt)
        .map_err(|e| AudioError::ProcessingError(e.to_string()))?;

    Ok(Resampler {
        source_hz: source_hz as f64,
        dest_hz: dest_hz as f64,
        bit_depth: bit_depth,
        soxr: soxr,
    })
}

pub fn resampling(left: &[i32], right: &[i32], resampler: &mut Resampler) -> (Vec<i32>, Vec<i32>) {
    if left.len() != right.len() {
        panic!("Left and right channels must have the same length");
    }
    let mut input = vec![[0.0f32; 2]; left.len()];
    match resampler.bit_depth {
        8 => i32_stereo_to_pairs_f32::<Bit8>(left, right, &mut input),
        16 => i32_stereo_to_pairs_f32::<Bit16>(left, right, &mut input),
        24 => i32_stereo_to_pairs_f32::<Bit24>(left, right, &mut input),
        32 => i32_stereo_to_pairs_f32::<Bit32>(left, right, &mut input),
        _ => panic!("Unsupported bit depth: {}", resampler.bit_depth),
    }

    let output_len =
        ((input.len() as f64) * resampler.dest_hz / resampler.source_hz).ceil() as usize;
    let mut output = vec![[0.0f32; 2]; output_len];

    resampler.soxr.process(&input, &mut output).unwrap();

    let mut oleft = vec![0i32; output.len()];
    let mut oright = vec![0i32; output.len()];

    match resampler.bit_depth {
        8 => pairs_f32_to_i32_stereo::<Bit8>(&output, &mut oleft, &mut oright),
        16 => pairs_f32_to_i32_stereo::<Bit16>(&output, &mut oleft, &mut oright),
        24 => pairs_f32_to_i32_stereo::<Bit24>(&output, &mut oleft, &mut oright),
        32 => pairs_f32_to_i32_stereo::<Bit32>(&output, &mut oleft, &mut oright),
        _ => unreachable!(), // Déjà vérifié plus haut
    };

    (oleft, oright)
}
