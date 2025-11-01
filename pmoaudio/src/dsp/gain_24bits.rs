use crate::I24;

/// Applique un gain (en dB) sur des échantillons stéréo interleavés `[L,R]`
/// codés sur 24 bits signés (`I24`).
pub fn apply_gain_stereo_i24(samples: &mut [[I24; 2]], gain_db: f64) {
    let gain = 10f64.powf(gain_db / 20.0);
    // Q23 scaling
    let g_q23 = (gain * (1u64 << 23) as f64).round() as i32;

    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    unsafe {
        apply_gain_stereo_i24_neon(samples, g_q23);
        return;
    }
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    unsafe {
        apply_gain_stereo_i24_avx2(samples, g_q23);
        return;
    }

    // Fallback scalaire
    #[cfg(not(any(
        all(target_arch = "aarch64", target_feature = "neon"),
        all(target_arch = "x86_64", target_feature = "avx2")
    )))]
    {
        apply_gain_stereo_i24_scalar(samples, g_q23);
    }
}

#[cfg(not(any(
    all(target_arch = "aarch64", target_feature = "neon"),
    all(target_arch = "x86_64", target_feature = "avx2")
)))]
#[inline(always)]
fn apply_gain_stereo_i24_scalar(samples: &mut [[I24; 2]], g_q23: i32) {
    for frame in samples.iter_mut() {
        // L
        let prod_l = (frame[0].as_i32() as i64 * g_q23 as i64 + (1 << 22)) >> 23;
        let clamped_l = prod_l.clamp(I24::MIN_VALUE as i64, I24::MAX_VALUE as i64) as i32;
        frame[0] = I24::new_clamped(clamped_l);

        // R
        let prod_r = (frame[1].as_i32() as i64 * g_q23 as i64 + (1 << 22)) >> 23;
        let clamped_r = prod_r.clamp(I24::MIN_VALUE as i64, I24::MAX_VALUE as i64) as i32;
        frame[1] = I24::new_clamped(clamped_r);
    }
}

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
#[inline(always)]
unsafe fn apply_gain_stereo_i24_neon(samples: &mut [[I24; 2]], g_q23: i32) {
    use core::arch::aarch64::*;
    let gvec = vdupq_n_s32(g_q23);
    let mut i = 0;
    let n = samples.len() * 2;
    let ptr = samples.as_mut_ptr() as *mut i32;

    while i + 4 <= n {
        let v = vld1q_s32(ptr.add(i));
        let res = vqdmulhq_s32(v, gvec); // Q23 multiply high
        vst1q_s32(ptr.add(i), res);
        i += 4;
    }

    // reste scalaire
    let slice = std::slice::from_raw_parts_mut(ptr.add(i), n - i);
    apply_gain_i24_scalar(slice, g_q23);
}

#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline(always)]
unsafe fn apply_gain_stereo_i24_avx2(samples: &mut [[I24; 2]], g_q23: i32) {
    use core::arch::x86_64::*;
    let g = _mm256_set1_epi32(g_q23);
    let mut i = 0;
    let n = samples.len() * 2;
    let ptr = samples.as_mut_ptr() as *mut i32;

    while i + 8 <= n {
        let x = _mm256_loadu_si256(ptr.add(i) as *const __m256i);
        let hi = _mm256_mulhi_epi32(x, g);
        _mm256_storeu_si256(ptr.add(i) as *mut __m256i, hi);
        i += 8;
    }

    // reste scalaire
    let slice = std::slice::from_raw_parts_mut(ptr.add(i), n - i);
    apply_gain_i24_scalar(slice, g_q23);
}

/// Version mono utilisée pour le reste scalaire.
#[inline(always)]
fn apply_gain_i24_scalar(samples: &mut [i32], g_q23: i32) {
    for s in samples.iter_mut() {
        let prod = (*s as i64 * g_q23 as i64 + (1 << 22)) >> 23;
        *s = prod.clamp(I24::MIN_VALUE as i64, I24::MAX_VALUE as i64) as i32;
    }
}
