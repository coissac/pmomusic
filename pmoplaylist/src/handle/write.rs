//! WriteHandle : accès exclusif en écriture à une playlist

use crate::playlist::core::PlaylistConfig;
use crate::playlist::record::Record;
use crate::playlist::Playlist;
use crate::Result;
use pmocache::cache_trait::FileCache;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Handle d'écriture sur une playlist (exclusif)
pub struct WriteHandle {
    playlist: Arc<Playlist>,
    _write_token: Arc<()>,
}

impl WriteHandle {
    /// Crée un nouveau handle d'écriture
    pub(crate) fn new(playlist: Arc<Playlist>, write_token: Arc<()>) -> Self {
        Self {
            playlist,
            _write_token: write_token,
        }
    }
    
    /// Ajoute un morceau à la playlist
    pub async fn push(&self, cache_pk: String) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }
        
        // Vérifier que le pk existe dans le cache
        let cache = crate::manager::audio_cache()?;
        if !cache.is_valid_pk(&cache_pk) {
            return Err(crate::Error::CacheEntryNotFound(cache_pk));
        }
        
        // Ajouter à la playlist
        let record = Record::new(cache_pk);
        let mut core = self.playlist.core.write().await;
        core.push(record);
        drop(core);
        
        self.playlist.touch().await;
        
        // Sauvegarder si persistante
        if self.playlist.persistent {
            self.save_to_db().await?;
        }
        
        Ok(())
    }
    
    /// Ajoute plusieurs morceaux de manière atomique
    pub async fn push_set(&self, cache_pks: Vec<String>) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }
        
        // Vérifier tous les pks d'abord
        let cache = crate::manager::audio_cache()?;
        for pk in &cache_pks {
            if !cache.is_valid_pk(pk) {
                return Err(crate::Error::CacheEntryNotFound(pk.clone()));
            }
        }
        
        // Créer tous les records
        let records: Vec<Record> = cache_pks.into_iter()
            .map(Record::new)
            .collect();
        
        // Ajouter atomiquement
        let mut core = self.playlist.core.write().await;
        core.push_all(records);
        drop(core);
        
        self.playlist.touch().await;
        
        // Une seule sauvegarde pour tout le batch
        if self.playlist.persistent {
            self.save_to_db().await?;
        }
        
        Ok(())
    }
    
    /// Vide la playlist
    pub async fn flush(&self) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }
        
        let mut core = self.playlist.core.write().await;
        core.clear();
        drop(core);
        
        self.playlist.touch().await;
        
        if self.playlist.persistent {
            self.save_to_db().await?;
        }
        
        Ok(())
    }
    
    /// Supprime la playlist définitivement
    pub async fn delete(self) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }
        
        // Marquer comme supprimée
        self.playlist.mark_deleted();
        
        // Supprimer du manager
        crate::manager::delete_playlist_internal(&self.playlist.id).await?;
        
        Ok(())
    }
    
    /// Change le titre
    pub async fn set_title(&self, title: String) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }
        
        self.playlist.set_title(title).await;
        
        if self.playlist.persistent {
            self.save_to_db().await?;
        }
        
        Ok(())
    }
    
    /// Change la capacité maximale
    pub async fn set_capacity(&self, max_size: Option<usize>) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }
        
        let mut core = self.playlist.core.write().await;
        core.set_capacity(max_size);
        drop(core);
        
        self.playlist.touch().await;
        
        if self.playlist.persistent {
            self.save_to_db().await?;
        }
        
        Ok(())
    }
    
    /// Change le TTL par défaut
    pub async fn set_default_ttl(&self, ttl: Option<Duration>) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }
        
        let mut core = self.playlist.core.write().await;
        core.set_default_ttl(ttl);
        drop(core);
        
        self.playlist.touch().await;
        
        if self.playlist.persistent {
            self.save_to_db().await?;
        }
        
        Ok(())
    }
    
    /// Clone vers une nouvelle playlist persistante
    pub async fn clone_as_persistent(&self, new_id: String) -> Result<WriteHandle> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }
        
        // Récupérer les données actuelles
        let title = self.playlist.title().await;
        let core = self.playlist.core.read().await;
        let config = core.config.clone();
        let tracks = core.snapshot();
        drop(core);
        
        // Créer la nouvelle playlist persistante
        let manager = crate::manager::PlaylistManager();
        let mut new_handle = manager.create_persistent_playlist(new_id).await?;
        
        // Copier le titre et la config
        new_handle.set_title(title).await?;
        new_handle.set_capacity(config.max_size).await?;
        new_handle.set_default_ttl(config.default_ttl).await?;
        
        // Copier tous les morceaux
        let pks: Vec<String> = tracks.iter()
            .map(|r| r.cache_pk.clone())
            .collect();
        new_handle.push_set(pks).await?;
        
        Ok(new_handle)
    }
    
    // Métadonnées
    
    pub fn id(&self) -> &str {
        &self.playlist.id
    }
    
    pub async fn title(&self) -> String {
        self.playlist.title().await
    }
    
    pub fn is_persistent(&self) -> bool {
        self.playlist.persistent
    }
    
    pub async fn capacity(&self) -> Option<usize> {
        let core = self.playlist.core.read().await;
        core.config.max_size
    }
    
    pub async fn default_ttl(&self) -> Option<Duration> {
        let core = self.playlist.core.read().await;
        core.config.default_ttl
    }
    
    pub async fn len(&self) -> usize {
        let core = self.playlist.core.read().await;
        core.len()
    }
    
    pub async fn is_empty(&self) -> bool {
        let core = self.playlist.core.read().await;
        core.is_empty()
    }
    
    pub async fn last_change(&self) -> SystemTime {
        self.playlist.last_change().await
    }
    
    // Helpers internes
    
    async fn save_to_db(&self) -> Result<()> {
        let manager = crate::manager::PlaylistManager();
        let persistence = manager.persistence()
            .ok_or_else(|| crate::Error::PersistenceError("No persistence manager".into()))?;
        
        let title = self.playlist.title().await;
        let core = self.playlist.core.read().await;
        let config = &core.config;
        let tracks = &core.tracks;
        
        persistence.save_playlist(&self.playlist.id, &title, config, tracks).await
    }
}
