//! Extensions pour pmoaudio
//!
//! Cette crate fournit des nodes d'extension pour pmoaudio qui dépendent
//! d'autres crates du projet. Elle permet d'éviter les dépendances cycliques
//! en plaçant ces extensions en "bout de chaîne" de dépendances.
//!
//! # Features
//!
//! - `cache-sink` : Active le `FlacCacheSink` qui encode l'audio en FLAC et le stocke dans pmoaudiocache
//! - `playlist` : Active l'intégration avec pmoplaylist (sources et sinks)
//! - `all` : Active toutes les features d'un coup
//!
//! # Architecture
//!
//! Cette crate dépend de :
//! - `pmoaudio` : Types de base (AudioSegment, AudioError, etc.)
//! - `pmoaudiocache` (optionnel) : Cache audio pour le stockage FLAC
//! - `pmoflac` (optionnel) : Encodage FLAC
//! - `pmometadata` (optionnel) : Gestion des métadonnées
//! - `pmoplaylist` (optionnel) : Intégration playlist
//!
//! Aucune des crates ci-dessus ne dépend de `pmoaudio-ext`, évitant ainsi
//! tout cycle de dépendances.

#[cfg(any(feature = "cache-sink", feature = "http-stream"))]
pub mod sinks;

#[cfg(any(feature = "cache-sink", feature = "http-stream"))]
pub mod nodes;

#[cfg(feature = "playlist")]
pub mod sources;

// Re-exports pour faciliter l'utilisation
#[cfg(any(feature = "cache-sink", feature = "http-stream"))]
pub use sinks::*;

#[cfg(any(feature = "cache-sink", feature = "http-stream"))]
pub use nodes::*;

#[cfg(feature = "playlist")]
pub use sources::*;
