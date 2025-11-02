//! Nodes audio pour pmoaudiocache
//!
//! Ce module fournit des nodes audio spécialisés qui étendent pmoaudio
//! pour intégrer le cache audio.

pub mod flac_cache_sink;

pub use flac_cache_sink::{FlacCacheSink, FlacCacheSinkStats};
