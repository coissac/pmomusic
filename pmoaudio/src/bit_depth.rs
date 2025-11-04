//! Bit depth abstraction for audio processing.
//!
//! Provides both compile-time generic types (`Bit8`, `Bit16`, …)
//! and a dynamic `BitDepth` enum for runtime selection.

use std::fmt;

/// Trait implemented by compile-time bit-depth marker types.
pub trait BitDepthType {
    const BITS: u32;
    const MAX_VALUE: f32;
}

/// Compile-time bit depth markers
#[derive(Clone, Copy, Debug)]
pub struct Bit8;
#[derive(Clone, Copy, Debug)]
pub struct Bit16;
#[derive(Clone, Copy, Debug)]
pub struct Bit24;
#[derive(Clone, Copy, Debug)]
pub struct Bit32;

impl BitDepthType for Bit8 {
    const BITS: u32 = 8;
    const MAX_VALUE: f32 = 128.0;
}
impl BitDepthType for Bit16 {
    const BITS: u32 = 16;
    const MAX_VALUE: f32 = 32_768.0;
}
impl BitDepthType for Bit24 {
    const BITS: u32 = 24;
    const MAX_VALUE: f32 = 8_388_608.0;
}
impl BitDepthType for Bit32 {
    const BITS: u32 = 32;
    const MAX_VALUE: f32 = 2_147_483_648.0;
}

/// Runtime bit-depth descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BitDepth {
    B8,
    B16,
    B24,
    B32,
}

impl BitDepth {
    /// Returns the number of bits.
    #[inline(always)]
    pub const fn bits(self) -> u32 {
        match self {
            BitDepth::B8 => 8,
            BitDepth::B16 => 16,
            BitDepth::B24 => 24,
            BitDepth::B32 => 32,
        }
    }

    /// Returns the full-scale signed maximum value as `f32`.
    #[inline(always)]
    pub const fn max_value(self) -> f32 {
        match self {
            BitDepth::B8 => 128.0,
            BitDepth::B16 => 32_768.0,
            BitDepth::B24 => 8_388_608.0,
            BitDepth::B32 => 2_147_483_648.0,
        }
    }

    /// Create from bit count, returning `None` if unsupported.
    #[inline(always)]
    pub const fn from_u32(bits: u32) -> Option<Self> {
        match bits {
            8 => Some(Self::B8),
            16 => Some(Self::B16),
            24 => Some(Self::B24),
            32 => Some(Self::B32),
            _ => None,
        }
    }

    /// Create from bit count, panicking if unsupported (non-const).
    #[inline(always)]
    pub fn from_u32_strict(bits: u32) -> Self {
        Self::from_u32(bits).unwrap_or_else(|| panic!("Unsupported bit depth: {}", bits))
    }
}

impl fmt::Display for BitDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-bit", self.bits())
    }
}

/// Comparaisons d’ordre fondées sur la valeur en bits.
impl PartialOrd for BitDepth {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BitDepth {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.bits().cmp(&other.bits())
    }
}

/// Bridge between dynamic [`BitDepth`] and static [`BitDepthType`] markers.
///
/// Example:
/// ```
/// use pmoaudio::{
///     bit_depth::dispatch_by_bitdepth, Bit8, Bit16, Bit24, Bit32, BitDepth,
/// };
/// use pmoaudio::bit_depth::BitDepthType;
///
/// fn type_bits<B: BitDepthType>() -> u32 {
///     B::BITS
/// }
///
/// let depth = BitDepth::B16;
/// let bits = dispatch_by_bitdepth(
///     depth,
///     || type_bits::<Bit8>(),
///     || type_bits::<Bit16>(),
///     || type_bits::<Bit24>(),
///     || type_bits::<Bit32>(),
/// );
/// assert_eq!(bits, 16);
/// ```
#[inline(always)]
pub fn dispatch_by_bitdepth<R, F8, F16, F24, F32>(
    depth: BitDepth,
    f8: F8,
    f16: F16,
    f24: F24,
    f32: F32,
) -> R
where
    F8: FnOnce() -> R,
    F16: FnOnce() -> R,
    F24: FnOnce() -> R,
    F32: FnOnce() -> R,
{
    match depth {
        BitDepth::B8 => f8(),
        BitDepth::B16 => f16(),
        BitDepth::B24 => f24(),
        BitDepth::B32 => f32(),
    }
}

/// Conversion helper from a runtime [`BitDepth`] to a compile-time constant.
#[inline(always)]
pub fn max_value_for(depth: BitDepth) -> f32 {
    depth.max_value()
}
