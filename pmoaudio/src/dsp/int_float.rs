use bytemuck::{cast_slice, cast_slice_mut};
use crate::BitDepth;

#[cfg(feature = "simd")]
use std::simd::num::{SimdFloat, SimdInt};
#[cfg(feature = "simd")]
use std::simd::{Simd, StdFloat};

/* ====================== CŒURS CANONIQUES EN AoS ====================== */

// i32 L/R -> [[f32;2]] - version interne avec constante compile-time
#[cfg(feature = "simd")]
fn i32_stereo_to_pairs_f32_inner(
    left: &[i32],
    right: &[i32],
    out_pairs: &mut [[f32; 2]],
    max_value: f32,
) {
    debug_assert_eq!(left.len(), right.len());
    debug_assert_eq!(out_pairs.len(), left.len());

    const LANES: usize = 8;
    type Vf32 = Simd<f32, LANES>;
    type Vi32 = Simd<i32, LANES>;

    let scale = Vf32::splat(1.0 / max_value);

    let (l_chunks, l_tail) = left.as_chunks::<LANES>();
    let (r_chunks, r_tail) = right.as_chunks::<LANES>();
    let (o_chunks, o_tail) = out_pairs.as_chunks_mut::<LANES>();

    for (k, o) in o_chunks.iter_mut().enumerate() {
        let l = Vi32::from_slice(&l_chunks[k]).cast::<f32>() * scale;
        let r = Vi32::from_slice(&r_chunks[k]).cast::<f32>() * scale;

        for j in 0..LANES {
            // AoS direct
            unsafe {
                *o.get_unchecked_mut(j) = [l[j], r[j]];
            }
        }
    }

    let scale_scalar = 1.0 / max_value;
    for (dst, (&l, &r)) in o_tail.iter_mut().zip(l_tail.iter().zip(r_tail.iter())) {
        dst[0] = l as f32 * scale_scalar;
        dst[1] = r as f32 * scale_scalar;
    }
}

#[cfg(not(feature = "simd"))]
fn i32_stereo_to_pairs_f32_inner(
    left: &[i32],
    right: &[i32],
    out_pairs: &mut [[f32; 2]],
    max_value: f32,
) {
    debug_assert_eq!(left.len(), right.len());
    debug_assert_eq!(out_pairs.len(), left.len());

    let scale = 1.0 / max_value;
    for ((out, &l), &r) in out_pairs.iter_mut().zip(left).zip(right) {
        out[0] = l as f32 * scale;
        out[1] = r as f32 * scale;
    }
}

/// Convertit deux canaux i32 (L/R) en pairs f32 normalisées [-1.0, 1.0]
pub fn i32_stereo_to_pairs_f32(
    left: &[i32],
    right: &[i32],
    out_pairs: &mut [[f32; 2]],
    bit_depth: BitDepth,
) {
    i32_stereo_to_pairs_f32_inner(left, right, out_pairs, bit_depth.max_value());
}

// [[f32;2]] -> i32 L/R - version interne
#[cfg(feature = "simd")]
fn pairs_f32_to_i32_stereo_inner(
    input_pairs: &[[f32; 2]],
    left: &mut [i32],
    right: &mut [i32],
    max_value: f32,
) {
    debug_assert_eq!(input_pairs.len(), left.len());
    debug_assert_eq!(input_pairs.len(), right.len());

    const LANES: usize = 8;
    type Vf32 = Simd<f32, LANES>;

    let vmin = -max_value;
    let vmax_clamp = max_value - 1.0; // évite l'overflow après round→cast
    let vscale = Vf32::splat(max_value);
    let vminv = Vf32::splat(vmin);
    let vmaxv = Vf32::splat(vmax_clamp);

    let (in_chunks, in_tail) = input_pairs.as_chunks::<LANES>();
    let (l_chunks, l_tail) = left.as_chunks_mut::<LANES>();
    let (r_chunks, r_tail) = right.as_chunks_mut::<LANES>();

    for (k, blk) in in_chunks.iter().enumerate() {
        // AoS → deux vecteurs f32
        let mut l_arr = [0.0f32; LANES];
        let mut r_arr = [0.0f32; LANES];
        for j in 0..LANES {
            let p = blk[j];
            l_arr[j] = p[0];
            r_arr[j] = p[1];
        }

        let lq = (Vf32::from_array(l_arr) * vscale)
            .simd_clamp(vminv, vmaxv)
            .round();
        let rq = (Vf32::from_array(r_arr) * vscale)
            .simd_clamp(vminv, vmaxv)
            .round();

        lq.cast::<i32>().copy_to_slice(&mut l_chunks[k]);
        rq.cast::<i32>().copy_to_slice(&mut r_chunks[k]);
    }

    for (j, (l, r)) in in_tail.iter().zip(l_tail.iter_mut().zip(r_tail.iter_mut())) {
        let lx = (j[0] * max_value).clamp(vmin, vmax_clamp).round();
        let rx = (j[1] * max_value).clamp(vmin, vmax_clamp).round();
        *l = lx as i32;
        *r = rx as i32;
    }
}

#[cfg(not(feature = "simd"))]
fn pairs_f32_to_i32_stereo_inner(
    input_pairs: &[[f32; 2]],
    left: &mut [i32],
    right: &mut [i32],
    max_value: f32,
) {
    debug_assert_eq!(input_pairs.len(), left.len());
    debug_assert_eq!(input_pairs.len(), right.len());

    let vmin = -max_value;
    let vmax_clamp = max_value - 1.0;
    for (i, pair) in input_pairs.iter().enumerate() {
        let lx = (pair[0] * max_value).clamp(vmin, vmax_clamp).round();
        let rx = (pair[1] * max_value).clamp(vmin, vmax_clamp).round();
        left[i] = lx as i32;
        right[i] = rx as i32;
    }
}

/// Convertit pairs f32 normalisées [-1.0, 1.0] en deux canaux i32 (L/R)
pub fn pairs_f32_to_i32_stereo(
    input_pairs: &[[f32; 2]],
    left: &mut [i32],
    right: &mut [i32],
    bit_depth: BitDepth,
) {
    pairs_f32_to_i32_stereo_inner(input_pairs, left, right, bit_depth.max_value());
}

/* ====================== WRAPPERS INTERLEAVÉS ====================== */

/// Convertit deux canaux i32 (L/R) en buffer f32 interleaved normalisé [-1.0, 1.0]
pub fn i32_stereo_to_interleaved_f32(
    left: &[i32],
    right: &[i32],
    out_interleaved: &mut [f32],
    bit_depth: BitDepth,
) {
    debug_assert_eq!(out_interleaved.len(), left.len() * 2);
    let out_pairs: &mut [[f32; 2]] = cast_slice_mut(out_interleaved);
    i32_stereo_to_pairs_f32(left, right, out_pairs, bit_depth);
}

/// Convertit buffer f32 interleaved normalisé [-1.0, 1.0] en deux canaux i32 (L/R)
pub fn interleaved_f32_to_i32_stereo(
    input_interleaved: &[f32],
    left: &mut [i32],
    right: &mut [i32],
    bit_depth: BitDepth,
) {
    debug_assert_eq!(input_interleaved.len(), left.len() * 2);
    let input_pairs: &[[f32; 2]] = cast_slice(input_interleaved);
    pairs_f32_to_i32_stereo(input_pairs, left, right, bit_depth);
}
