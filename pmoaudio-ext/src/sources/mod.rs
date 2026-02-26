//! Sources audio étendues pour pmoaudio
//!
//! Ce module contient des sources audio qui dépendent d'autres crates
//! du projet PMO (pmoplaylist, pmoaudiocache, etc.)

// Helpers partagés (conversion PCM → AudioSegment)
// Disponibles dès que l'une des deux features qui en dépend est activée
#[cfg(any(feature = "playlist", feature = "http-stream"))]
pub(crate) mod pcm_decode;

#[cfg(feature = "playlist")]
mod playlist_source;

#[cfg(feature = "playlist")]
pub use playlist_source::PlaylistSource;

#[cfg(feature = "http-stream")]
mod uri_source;

#[cfg(feature = "http-stream")]
pub use uri_source::UriSource;
