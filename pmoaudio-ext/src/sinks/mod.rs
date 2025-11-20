//! Sinks d'extension pour pmoaudio
//!
//! Ce module contient des sinks qui dépendent de multiples crates
//! et ne peuvent pas être placés directement dans pmoaudio sans créer
//! de dépendances cycliques.

pub mod byte_stream_reader;
pub mod chunk_to_pcm;
pub mod streaming_icyflac_sink;

#[cfg(feature = "cache-sink")]
mod flac_cache_sink;

#[cfg(feature = "cache-sink")]
pub use flac_cache_sink::{FlacCacheSink, FlacCacheSinkStats, TrackStats};

#[cfg(feature = "http-stream")]
mod broadcast_pacing;

#[cfg(feature = "http-stream")]
mod flac_frame_utils;

#[cfg(feature = "http-stream")]
mod timed_broadcast;

#[cfg(feature = "http-stream")]
mod streaming_flac_sink;

#[cfg(feature = "http-stream")]
mod streaming_sink_common;

#[cfg(feature = "http-stream")]
pub use streaming_flac_sink::{FlacClientStream, StreamHandle, StreamingFlacSink};

#[cfg(feature = "http-stream")]
pub use streaming_icyflac_sink::IcyClientStream;

#[cfg(feature = "http-stream")]
mod streaming_ogg_flac_sink;

#[cfg(feature = "http-stream")]
pub use streaming_ogg_flac_sink::{OggFlacClientStream, OggFlacStreamHandle, StreamingOggFlacSink};

#[cfg(feature = "http-stream")]
pub use streaming_sink_common::MetadataSnapshot;
