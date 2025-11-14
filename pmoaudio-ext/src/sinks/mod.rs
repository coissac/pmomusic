//! Sinks d'extension pour pmoaudio
//!
//! Ce module contient des sinks qui dépendent de multiples crates
//! et ne peuvent pas être placés directement dans pmoaudio sans créer
//! de dépendances cycliques.

#[cfg(feature = "cache-sink")]
mod flac_cache_sink;

#[cfg(feature = "cache-sink")]
pub use flac_cache_sink::{FlacCacheSink, FlacCacheSinkStats, TrackStats};

#[cfg(feature = "http-stream")]
mod broadcast_pacing;

#[cfg(feature = "http-stream")]
mod flac_frame_utils;

#[cfg(feature = "http-stream")]
mod streaming_flac_sink;

#[cfg(feature = "http-stream")]
pub use streaming_flac_sink::{
    FlacClientStream, IcyClientStream, MetadataSnapshot, StreamHandle, StreamingFlacSink,
};

#[cfg(feature = "http-stream")]
mod streaming_ogg_flac_sink;

#[cfg(feature = "http-stream")]
pub use streaming_ogg_flac_sink::{OggFlacClientStream, OggFlacStreamHandle, StreamingOggFlacSink};
