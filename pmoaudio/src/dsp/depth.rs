//! Conversion de bit-depth pour audio stéréo avec optimisations SIMD
//!
//! Ce module fournit des fonctions pour convertir la profondeur de bit (bit-depth)
//! des échantillons audio stéréo. Les conversions sont optimisées avec SIMD lorsque
//! la feature "simd" est activée.
//!
//! # Stratégies de conversion
//!
//! - **Augmentation de bit-depth** : Décalage à gauche (multiplication par 2^n)
//!   - Exemple : 16-bit → 24-bit : shift left de 8 bits
//!   - Préserve la dynamique complète du signal
//!
//! - **Réduction de bit-depth** : Décalage à droite avec clamping
//!   - Exemple : 24-bit → 16-bit : shift right de 8 bits
//!   - Clamp aux limites du format destination pour éviter l'overflow
//!   - Division entière (pas d'arrondi) pour les performances
//!
//! # Optimisations SIMD
//!
//! Quand la feature "simd" est activée, les conversions traitent 8 frames stéréo
//! en parallèle en utilisant les instructions vectorielles SIMD disponibles.

#[cfg(feature = "simd")]
use std::simd::prelude::*;
#[cfg(feature = "simd")]
use std::simd::Simd;

use crate::BitDepth;

/// Convertit la profondeur de bit (bit-depth) d'un buffer audio stéréo
///
/// Cette fonction modifie les échantillons in-place pour convertir de `source_bits`
/// vers `dest_bits`. Si les profondeurs sont identiques, aucune opération n'est effectuée.
///
/// # Arguments
///
/// * `data` - Buffer mutable de frames stéréo [[L, R], ...] à convertir in-place
/// * `source_bits` - Profondeur de bit source (16, 24, ou 32 bits)
/// * `dest_bits` - Profondeur de bit destination (16, 24, ou 32 bits)
///
/// # Exemples
///
/// ```
/// use pmoaudio::dsp::bitdepth_change_stereo;
/// use pmoaudio::BitDepth;
///
/// let mut data = vec![[1000i32, 2000i32], [3000i32, 4000i32]];
/// bitdepth_change_stereo(&mut data, BitDepth::B16, BitDepth::B24);
/// // Les valeurs sont maintenant décalées de 8 bits à gauche
/// assert_eq!(data[0], [256000, 512000]);
/// ```
#[inline(always)]
pub fn bitdepth_change_stereo(data: &mut [[i32; 2]], source_bits: BitDepth, dest_bits: BitDepth) {
    use std::cmp::Ordering::*;
    let obits = dest_bits.bits();
    let ibits = source_bits.bits();
    match source_bits.cmp(&dest_bits) {
        Less => bitdepth_up_stereo(data, (obits - ibits) as i32),
        Greater => bitdepth_down_stereo(data, (ibits - obits) as i32, obits),
        Equal => (),
    }
}

#[inline(always)]
#[cfg(feature = "simd")]
fn bitdepth_up_stereo(data: &mut [[i32; 2]], shift: i32) {
    const LANES: usize = 8;
    let shift_vec = Simd::<i32, LANES>::splat(shift);

    // On traite 8 frames stéréo à la fois
    let (chunks, remainder) = data.as_chunks_mut::<LANES>();
    for blk in chunks {
        // Séparer L et R localement (petit tableau sur la pile)
        let mut l = [0i32; LANES];
        let mut r = [0i32; LANES];
        for j in 0..LANES {
            let s = blk[j];
            l[j] = s[0];
            r[j] = s[1];
        }

        // SIMD
        let vl = Simd::<i32, LANES>::from_array(l) << shift_vec;
        let vr = Simd::<i32, LANES>::from_array(r) << shift_vec;

        // Écrire
        for j in 0..LANES {
            blk[j] = [vl[j], vr[j]];
        }
    }

    // Reste scalaire
    for f in remainder {
        f[0] <<= shift;
        f[1] <<= shift;
    }
}

#[inline(always)]
#[cfg(not(feature = "simd"))]
fn bitdepth_up_stereo(data: &mut [[i32; 2]], shift: i32) {
    for frame in data.iter_mut() {
        frame[0] <<= shift;
        frame[1] <<= shift;
    }
}

#[inline(always)]
#[cfg(feature = "simd")]
fn bitdepth_down_stereo(data: &mut [[i32; 2]], shift: i32, dest_bits: u32) {
    const LANES: usize = 8;
    let shift_vec = Simd::<i32, LANES>::splat(shift);
    let maxv = Simd::<i32, LANES>::splat(((1i64 << (dest_bits - 1)) - 1) as i32);
    let minv = Simd::<i32, LANES>::splat((-(1i64 << (dest_bits - 1))) as i32);

    let (chunks, remainder) = data.as_chunks_mut::<LANES>();
    for blk in chunks {
        let mut l = [0i32; LANES];
        let mut r = [0i32; LANES];
        for j in 0..LANES {
            l[j] = blk[j][0];
            r[j] = blk[j][1];
        }

        let vl = Simd::<i32, LANES>::from_array(l);
        let vr = Simd::<i32, LANES>::from_array(r);

        let lq = (vl >> shift_vec).simd_clamp(minv, maxv);
        let rq = (vr >> shift_vec).simd_clamp(minv, maxv);

        for j in 0..LANES {
            blk[j] = [lq[j], rq[j]];
        }
    }

    // Reste scalaire
    for f in remainder {
        f[0] = ((*f)[0] as i64 >> shift)
            .clamp(-(1i64 << (dest_bits - 1)), (1i64 << (dest_bits - 1)) - 1) as i32;
        f[1] = ((*f)[1] as i64 >> shift)
            .clamp(-(1i64 << (dest_bits - 1)), (1i64 << (dest_bits - 1)) - 1) as i32;
    }
}

#[inline(always)]
#[cfg(not(feature = "simd"))]
fn bitdepth_down_stereo(data: &mut [[i32; 2]], shift: i32, dest_bits: u32) {
    let maxv = (1i64 << (dest_bits - 1)) - 1;
    let minv = -(1i64 << (dest_bits - 1));

    for frame in data.iter_mut() {
        frame[0] = ((frame[0] as i64 >> shift).clamp(minv, maxv)) as i32;
        frame[1] = ((frame[1] as i64 >> shift).clamp(minv, maxv)) as i32;
    }
}
