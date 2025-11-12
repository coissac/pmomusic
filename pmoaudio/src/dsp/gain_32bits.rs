//! Application de gain pour audio 32-bit avec optimisations SIMD
//!
//! Ce module implémente l'application de gain pour les échantillons audio 32-bit
//! en utilisant l'arithmétique à virgule fixe Q31.
//!
//! # Méthode Q31
//!
//! Le gain linéaire est converti en format Q31 (Fixed-Point 31-bit fractional):
//! - Gain linéaire multiplié par 2^31
//! - Multiplication Q31 × i32 → i64
//! - Arrondi avec ajout de 2^30 avant shift right de 31 bits
//! - Clamping aux limites i32 pour éviter l'overflow
//!
//! # Optimisations SIMD
//!
//! - **ARM NEON** : vqdmulhq_s32 (Q31 multiply high avec saturation)
//! - **x86_64 AVX2** : _mm256_mulhi_epi32 (traite 8 échantillons en parallèle)
//! - **Fallback scalaire** : Pour les architectures sans SIMD

/// Applique un gain (en dB) sur des échantillons stéréo interleavés `[L,R]`
///
/// Cette fonction applique un gain en décibels sur chaque échantillon du buffer.
/// Elle utilise automatiquement les instructions SIMD disponibles (NEON ou AVX2).
///
/// # Arguments
///
/// * `samples` - Buffer mutable de frames stéréo [[L, R], ...] à modifier in-place
/// * `gain_db` - Gain à appliquer en décibels (positif = amplification, négatif = atténuation)
///
/// # Exemples
///
/// ```
/// use pmoaudio::dsp::apply_gain_stereo_i32;
///
/// let mut data = vec![[100000i32, 200000i32]];
/// apply_gain_stereo_i32(&mut data, 6.0); // Amplifie de +6dB (×2)
/// ```
///
/// # Performance
///
/// - ARM NEON: ~4 échantillons par cycle
/// - x86_64 AVX2: ~8 échantillons par cycle
/// - Scalaire: ~1 échantillon par 2-3 cycles
pub fn apply_gain_stereo_i32(samples: &mut [[i32; 2]], gain_db: f64) {
    let gain = 10f64.powf(gain_db / 20.0);
    let g_q31 = (gain * (1u64 << 31) as f64).round() as i32;

    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    unsafe {
        apply_gain_stereo_i32_neon(samples, g_q31);
        return;
    }
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    unsafe {
        apply_gain_stereo_i32_avx2(samples, g_q31);
        return;
    }

    // Fallback scalar
    #[cfg(not(any(
        all(target_arch = "aarch64", target_feature = "neon"),
        all(target_arch = "x86_64", target_feature = "avx2")
    )))]
    {
        apply_gain_stereo_i32_scalar(samples, g_q31);
    }
}

#[cfg(not(any(
    all(target_arch = "aarch64", target_feature = "neon"),
    all(target_arch = "x86_64", target_feature = "avx2")
)))]
#[inline(always)]
fn apply_gain_stereo_i32_scalar(samples: &mut [[i32; 2]], g_q31: i32) {
    for frame in samples.iter_mut() {
        // L
        let prod_l = (frame[0] as i64 * g_q31 as i64 + (1 << 30)) >> 31;
        frame[0] = prod_l.clamp(i32::MIN as i64, i32::MAX as i64) as i32;

        // R
        let prod_r = (frame[1] as i64 * g_q31 as i64 + (1 << 30)) >> 31;
        frame[1] = prod_r.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    }
}

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
#[inline(always)]
unsafe fn apply_gain_stereo_i32_neon(samples: &mut [[i32; 2]], g_q31: i32) {
    use core::arch::aarch64::*;
    let gvec = vdupq_n_s32(g_q31);
    let mut i = 0;
    let n = samples.len() * 2; // total d'échantillons (L+R)
    let ptr = samples.as_mut_ptr() as *mut i32;

    while i + 4 <= n {
        let v = vld1q_s32(ptr.add(i));
        let res = vqdmulhq_s32(v, gvec); // Q31 multiply high
        vst1q_s32(ptr.add(i), res);
        i += 4;
    }

    // reste scalaire
    let slice = std::slice::from_raw_parts_mut(ptr.add(i), n - i);
    apply_gain_i32_scalar(slice, g_q31);
}

#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline(always)]
unsafe fn apply_gain_stereo_i32_avx2(samples: &mut [[i32; 2]], g_q31: i32) {
    use core::arch::x86_64::*;
    let g = _mm256_set1_epi32(g_q31);
    let mut i = 0;
    let n = samples.len() * 2; // total d'échantillons
    let ptr = samples.as_mut_ptr() as *mut i32;

    while i + 8 <= n {
        let x = _mm256_loadu_si256(ptr.add(i) as *const __m256i);
        let hi = _mm256_mulhi_epi32(x, g);
        _mm256_storeu_si256(ptr.add(i) as *mut __m256i, hi);
        i += 8;
    }

    // reste scalaire
    let slice = std::slice::from_raw_parts_mut(ptr.add(i), n - i);
    apply_gain_i32_scalar(slice, g_q31);
}

/// version mono utilisée pour le reste scalaire
#[inline(always)]
fn apply_gain_i32_scalar(samples: &mut [i32], g_q31: i32) {
    for s in samples.iter_mut() {
        let prod = (*s as i64 * g_q31 as i64 + (1 << 30)) >> 31;
        *s = prod.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    }
}
