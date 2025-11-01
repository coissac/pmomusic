/// Applique un gain (en dB) sur des échantillons stéréo interleavés `[L,R]`
/// codés sur 16 bits signés.
pub fn apply_gain_stereo_i16(samples: &mut [[i16; 2]], gain_db: f64) {
    let gain = 10f64.powf(gain_db / 20.0);
    let g_q15 = (gain * (1u32 << 15) as f64).round() as i16;

    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    unsafe {
        apply_gain_stereo_i16_neon(samples, g_q15);
        return;
    }
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    unsafe {
        apply_gain_stereo_i16_avx2(samples, g_q15);
        return;
    }

    // Fallback scalaire
    #[cfg(not(any(
        all(target_arch = "aarch64", target_feature = "neon"),
        all(target_arch = "x86_64", target_feature = "avx2")
    )))]
    {
        apply_gain_stereo_i16_scalar(samples, g_q15);
    }
}

#[cfg(not(any(
    all(target_arch = "aarch64", target_feature = "neon"),
    all(target_arch = "x86_64", target_feature = "avx2")
)))]
#[inline(always)]
fn apply_gain_stereo_i16_scalar(samples: &mut [[i16; 2]], g_q15: i16) {
    for frame in samples.iter_mut() {
        // L
        let prod_l = (frame[0] as i32 * g_q15 as i32 + (1 << 14)) >> 15;
        frame[0] = prod_l.clamp(i16::MIN as i32, i16::MAX as i32) as i16;

        // R
        let prod_r = (frame[1] as i32 * g_q15 as i32 + (1 << 14)) >> 15;
        frame[1] = prod_r.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }
}

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
#[inline(always)]
unsafe fn apply_gain_stereo_i16_neon(samples: &mut [[i16; 2]], g_q15: i16) {
    use core::arch::aarch64::*;
    let gvec = vdupq_n_s16(g_q15);
    let mut i = 0;
    let n = samples.len() * 2;
    let ptr = samples.as_mut_ptr() as *mut i16;

    while i + 8 <= n {
        let v = vld1q_s16(ptr.add(i));
        let res = vqdmulhq_s16(v, gvec); // Q15 multiply high
        vst1q_s16(ptr.add(i), res);
        i += 8;
    }

    // reste scalaire
    let slice = std::slice::from_raw_parts_mut(ptr.add(i), n - i);
    apply_gain_i16_scalar(slice, g_q15);
}

#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline(always)]
unsafe fn apply_gain_stereo_i16_avx2(samples: &mut [[i16; 2]], g_q15: i16) {
    use core::arch::x86_64::*;
    let g = _mm256_set1_epi16(g_q15 as i16);
    let mut i = 0;
    let n = samples.len() * 2;
    let ptr = samples.as_mut_ptr() as *mut i16;

    while i + 16 <= n {
        let x = _mm256_loadu_si256(ptr.add(i) as *const __m256i);
        let hi = _mm256_mulhi_epi16(x, g);
        _mm256_storeu_si256(ptr.add(i) as *mut __m256i, hi);
        i += 16;
    }

    // reste scalaire
    let slice = std::slice::from_raw_parts_mut(ptr.add(i), n - i);
    apply_gain_i16_scalar(slice, g_q15);
}

/// version mono utilisée pour le reste scalaire
#[inline(always)]
fn apply_gain_i16_scalar(samples: &mut [i16], g_q15: i16) {
    for s in samples.iter_mut() {
        let prod = (*s as i32 * g_q15 as i32 + (1 << 14)) >> 15;
        *s = prod.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }
}
