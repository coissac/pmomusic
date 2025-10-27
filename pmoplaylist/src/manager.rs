//! PlaylistManager : gestionnaire singleton central de toutes les playlists

use crate::handle::{ReadHandle, WriteHandle};
use crate::persistence::PersistenceManager;
use crate::playlist::core::PlaylistConfig;
use crate::playlist::Playlist;
use crate::Result;
use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Singleton PlaylistManager
static PLAYLIST_MANAGER: OnceCell<PlaylistManager> = OnceCell::new();

/// Structure interne du manager
struct ManagerInner {
    playlists: RwLock<HashMap<String, Arc<Playlist>>>,
    persistence: Option<Arc<PersistenceManager>>,
}

/// Gestionnaire central de playlists
pub struct PlaylistManager {
    inner: Arc<ManagerInner>,
}

impl PlaylistManager {
    /// Initialise le gestionnaire (� appeler une seule fois au d�marrage)
    fn init(db_path: PathBuf) -> Result<Self> {
        // Initialiser la persistance
        let persistence = Arc::new(PersistenceManager::new(&db_path)?);

        let manager = Self {
            inner: Arc::new(ManagerInner {
                playlists: RwLock::new(HashMap::new()),
                persistence: Some(persistence.clone()),
            }),
        };

        // Lancer la task d'�viction en background
        let manager_clone = manager.clone();
        tokio::spawn(async move {
            manager_clone.eviction_task().await;
        });

        Ok(manager)
    }

    /// Initialise avec la configuration de pmoconfig
    #[cfg(feature = "pmoconfig")]
    fn init_with_config() -> Result<Self> {
        use crate::config_ext::PlaylistConfigExt;

        let config = pmoconfig::get_config();
        let db_path = config.playlist_db_path();

        Self::init(db_path)
    }

    /// Retourne le singleton
    pub fn get() -> &'static PlaylistManager {
        #[cfg(feature = "pmoconfig")]
        {
            PLAYLIST_MANAGER.get_or_init(|| {
                Self::init_with_config().expect("Failed to initialize PlaylistManager")
            })
        }

        #[cfg(not(feature = "pmoconfig"))]
        {
            PLAYLIST_MANAGER.get().expect("PlaylistManager not initialized. Call init() first.")
        }
    }

    /// Cr�e une playlist persistante (erreur si existe d�j�)
    pub async fn create_persistent_playlist(&self, id: String) -> Result<WriteHandle> {
        let mut playlists = self.inner.playlists.write().await;

        if playlists.contains_key(&id) {
            return Err(crate::Error::PlaylistAlreadyExists(id));
        }

        let playlist = Arc::new(Playlist::new(
            id.clone(),
            id.clone(), // Titre = id par d�faut
            PlaylistConfig::default(),
            true, // persistent
        ));

        // Acqu�rir le write lock
        let write_token = playlist
            .acquire_write_lock()
            .await
            .map_err(|_| crate::Error::WriteLockHeld(id.clone()))?;

        playlists.insert(id.clone(), playlist.clone());
        drop(playlists);

        // Sauvegarder la structure vide
        if let Some(persistence) = &self.inner.persistence {
            let title = playlist.title().await;
            let core = playlist.core.read().await;
            persistence.save_playlist(&playlist.id, &title, &core.config, &core.tracks)
                .await?;
        }

        Ok(WriteHandle::new(playlist, write_token))
    }

    /// R�cup�re un write handle (cr�e �ph�m�re si n'existe pas)
    pub async fn get_write_handle(&self, id: String) -> Result<WriteHandle> {
        let mut playlists = self.inner.playlists.write().await;

        if let Some(playlist) = playlists.get(&id) {
            // Playlist existe, tenter d'acqu�rir le lock
            let write_token = playlist
                .acquire_write_lock()
                .await
                .map_err(|_| crate::Error::WriteLockHeld(id.clone()))?;

            return Ok(WriteHandle::new(playlist.clone(), write_token));
        }

        // N'existe pas, cr�er �ph�m�re
        let playlist = Arc::new(Playlist::new(
            id.clone(),
            id.clone(),
            PlaylistConfig::default(),
            false, // �ph�m�re
        ));

        let write_token = playlist
            .acquire_write_lock()
            .await
            .map_err(|_| crate::Error::WriteLockHeld(id.clone()))?;

        playlists.insert(id, playlist.clone());
        drop(playlists);

        Ok(WriteHandle::new(playlist, write_token))
    }

    /// R�cup�re un write handle persistant (cr�e si n'existe pas)
    pub async fn get_persistent_write_handle(&self, id: String) -> Result<WriteHandle> {
        let playlists = self.inner.playlists.read().await;

        if let Some(playlist) = playlists.get(&id) {
            // Playlist existe
            if !playlist.persistent {
                return Err(crate::Error::PlaylistNotPersistent(id));
            }

            let write_token = playlist
                .acquire_write_lock()
                .await
                .map_err(|_| crate::Error::WriteLockHeld(id.clone()))?;

            return Ok(WriteHandle::new(playlist.clone(), write_token));
        }

        drop(playlists);

        // N'existe pas, cr�er persistent
        self.create_persistent_playlist(id).await
    }

    /// R�cup�re un read handle (ressuscite depuis DB si besoin)
    pub async fn get_read_handle(&self, id: &str) -> Result<ReadHandle> {
        let playlists = self.inner.playlists.read().await;

        if let Some(playlist) = playlists.get(id) {
            if !playlist.is_alive() {
                return Err(crate::Error::PlaylistDeleted(id.to_string()));
            }
            return Ok(ReadHandle::new(playlist.clone()));
        }

        drop(playlists);

        // Pas en m�moire, essayer de ressusciter depuis la DB
        if let Some(persistence) = &self.inner.persistence {
            if let Some((title, config, tracks)) = persistence.load_playlist(id).await? {
                // Reconstruire la playlist
                let mut playlists = self.inner.playlists.write().await;

                let playlist = Arc::new(Playlist::new(
                    id.to_string(),
                    title.clone(),
                    config,
                    true,
                ));

                // Restaurer les tracks
                {
                    let mut core = playlist.core.write().await;
                    core.tracks = tracks;
                }

                playlists.insert(id.to_string(), playlist.clone());
                drop(playlists);

                return Ok(ReadHandle::new(playlist));
            }
        }

        Err(crate::Error::PlaylistNotFound(id.to_string()))
    }

    /// Supprime une playlist d�finitivement
    pub async fn delete_playlist(&self, id: &str) -> Result<()> {
        let mut playlists = self.inner.playlists.write().await;

        if let Some(playlist) = playlists.remove(id) {
            playlist.mark_deleted();
        }

        drop(playlists);

        // Supprimer de la DB
        if let Some(persistence) = &self.inner.persistence {
            persistence.delete_playlist(id).await?;
        }

        Ok(())
    }

    /// Liste toutes les playlists
    pub async fn list_playlists(&self) -> Vec<String> {
        let playlists = self.inner.playlists.read().await;
        playlists.keys().cloned().collect()
    }

    /// V�rifie si une playlist existe
    pub async fn exists(&self, id: &str) -> bool {
        self.inner.playlists.read().await.contains_key(id)
    }

    /// Retourne la r�f�rence au PersistenceManager
    pub(crate) fn persistence(&self) -> Option<&Arc<PersistenceManager>> {
        self.inner.persistence.as_ref()
    }

    /// Task d'�viction en background
    async fn eviction_task(&self) {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;

            let playlists = self.inner.playlists.read().await;

            for playlist in playlists.values() {
                if !playlist.is_alive() {
                    continue;
                }

                let mut core = playlist.core.write().await;
                let initial_len = core.len();
                core.evict();
                let new_len = core.len();
                drop(core);

                // Si des morceaux ont �t� �vict�s et la playlist est persistante
                if new_len < initial_len && playlist.persistent {
                    if let Some(persistence) = &self.inner.persistence {
                        let title = playlist.title().await;
                        let core = playlist.core.read().await;
                        let _ = persistence.save_playlist(&playlist.id, &title, &core.config, &core.tracks).await;
                    }
                }
            }
        }
    }

    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Helper pour supprimer une playlist (appel� depuis WriteHandle)
pub(crate) async fn delete_playlist_internal(id: &str) -> Result<()> {
    PlaylistManager::get().delete_playlist(id).await
}

/// Helper pour acc�der au cache audio
pub(crate) fn audio_cache() -> Result<Arc<pmoaudiocache::Cache>> {
    pmoupnp::get_audio_cache()
        .ok_or_else(|| crate::Error::ManagerNotInitialized)
}

/// Fonction raccourcie pour acc�der au singleton
pub fn PlaylistManager() -> &'static PlaylistManager {
    PlaylistManager::get()
}
