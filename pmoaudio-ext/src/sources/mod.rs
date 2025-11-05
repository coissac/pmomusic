//! Sources audio étendues pour pmoaudio
//!
//! Ce module contient des sources audio qui dépendent d'autres crates
//! du projet PMO (pmoplaylist, pmoaudiocache, etc.)

#[cfg(feature = "playlist")]
mod playlist_source;

#[cfg(feature = "playlist")]
pub use playlist_source::PlaylistSource;
