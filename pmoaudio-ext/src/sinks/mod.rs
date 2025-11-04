//! Sinks d'extension pour pmoaudio
//!
//! Ce module contient des sinks qui dépendent de multiples crates
//! et ne peuvent pas être placés directement dans pmoaudio sans créer
//! de dépendances cycliques.

#[cfg(feature = "cache-sink")]
mod flac_cache_sink;

#[cfg(feature = "cache-sink")]
pub use flac_cache_sink::{FlacCacheSink, FlacCacheSinkStats, TrackStats};
