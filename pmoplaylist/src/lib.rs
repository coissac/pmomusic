//! # pmoplaylist - Gestionnaire centralisé de playlists FIFO multi-consommateurs
//!
//! Cette crate fournit un gestionnaire centralisé de playlists avec :
//! - Gestion FIFO avec capacité et TTL configurables
//! - Multi-consommateurs indépendants
//! - Persistance optionnelle (SQLite)
//! - Intégration avec pmoaudiocache
//! - Génération DIDL-Lite pour UPnP
//!
//! # Architecture
//!
//! - **PlaylistManager** : Singleton central gérant toutes les playlists
//! - **WriteHandle** : Accès exclusif en écriture (push, flush, delete)
//! - **ReadHandle** : Accès en lecture avec curseur individuel (pop, peek)
//! - **PlaylistTrack** : Référence minimale vers pmoaudiocache
//!
//! # Exemple d'utilisation
//!
//! ```no_run
//! use pmoplaylist::PlaylistManager;
//!
//! # #[tokio::main]
//! # async fn main() -> pmoplaylist::Result<()> {
//! // Obtenir le gestionnaire (init automatique avec pmoconfig)
//! let manager = PlaylistManager();
//!
//! // Créer une playlist persistante
//! let mut writer = manager.create_persistent_playlist("radio-paradise".into())?;
//! writer.set_title("Radio Paradise - Main Mix".into()).await?;
//!
//! // Ajouter des morceaux (par cache_pk)
//! writer.push("abc123".into()).await?;
//! writer.push("def456".into()).await?;
//!
//! // Créer un consommateur
//! let mut reader = manager.get_read_handle("radio-paradise")?;
//!
//! // Consommer
//! while let Some(track) = reader.pop().await? {
//!     let path = track.file_path()?;
//!     println!("Playing: {:?}", path);
//! }
//! # Ok(())
//! # }
//! ```

mod error;
mod handle;
mod manager;
mod persistence;
mod playlist;
mod track;

#[cfg(feature = "pmoconfig")]
mod config_ext;

// Réexports publics
pub use error::{Error, Result};
pub use handle::{ReadHandle, WriteHandle};
pub use manager::{PlaylistManager, PlaylistManager as Manager};
pub use track::PlaylistTrack;

#[cfg(feature = "pmoconfig")]
pub use config_ext::PlaylistConfigExt;
