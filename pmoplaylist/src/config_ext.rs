//! Extension de pmoconfig pour les playlists

use std::path::PathBuf;

/// Trait d'extension pour pmoconfig::Config
pub trait PlaylistConfigExt {
    /// Retourne le chemin de la base de données des playlists
    fn playlist_db_path(&self) -> PathBuf;
}

impl PlaylistConfigExt for pmoconfig::Config {
    fn playlist_db_path(&self) -> PathBuf {
        // Utilise get_managed_dir pour créer le répertoire playlists s'il n'existe pas
        let playlists_dir = self
            .get_managed_dir(&["playlists", "directory"], "playlists")
            .expect("Failed to get or create playlists directory");

        PathBuf::from(playlists_dir).join("playlists.db")
    }
}
