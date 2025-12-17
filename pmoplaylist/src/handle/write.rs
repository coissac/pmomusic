//! WriteHandle : accès exclusif en écriture à une playlist

use crate::playlist::record::Record;
use crate::playlist::{Playlist, PlaylistRole};
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
        if !cache.is_valid_pk(&cache_pk).await {
            return Err(crate::Error::CacheEntryNotFound(cache_pk));
        }

        // Ajouter à la playlist
        let record = Record::new(cache_pk);
        let mut core = self.playlist.core.write().await;
        core.push(record);
        let snapshot = core.snapshot();
        drop(core);

        self.playlist.touch().await;

        // Sauvegarder si persistante
        if self.playlist.persistent {
            self.save_to_db().await?;
        }

        // Notifier le manager
        let manager = crate::manager::PlaylistManager();
        manager
            .rebuild_track_index(&self.playlist.id, &snapshot)
            .await;
        manager.notify_playlist_changed(&self.playlist.id);

        Ok(())
    }

    /// Ajoute un morceau avec un TTL personnalisé
    pub async fn push_with_ttl(&self, cache_pk: String, ttl: Duration) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        // Vérifier que le pk existe dans le cache
        let cache = crate::manager::audio_cache()?;
        if !cache.is_valid_pk(&cache_pk).await {
            return Err(crate::Error::CacheEntryNotFound(cache_pk));
        }

        // Ajouter à la playlist avec TTL
        let record = Record::with_ttl(cache_pk, ttl);
        let mut core = self.playlist.core.write().await;
        core.push(record);
        let snapshot = core.snapshot();
        drop(core);

        self.playlist.touch().await;

        // Sauvegarder si persistante
        if self.playlist.persistent {
            self.save_to_db().await?;
        }

        let manager = crate::manager::PlaylistManager();
        manager
            .rebuild_track_index(&self.playlist.id, &snapshot)
            .await;
        manager.notify_playlist_changed(&self.playlist.id);

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
            if !cache.is_valid_pk(pk).await {
                return Err(crate::Error::CacheEntryNotFound(pk.clone()));
            }
        }

        // Créer tous les records
        let records: Vec<Record> = cache_pks.into_iter().map(Record::new).collect();

        // Ajouter atomiquement
        let mut core = self.playlist.core.write().await;
        core.push_all(records);
        let snapshot = core.snapshot();
        drop(core);

        self.playlist.touch().await;

        // Une seule sauvegarde pour tout le batch
        if self.playlist.persistent {
            self.save_to_db().await?;
        }

        let manager = crate::manager::PlaylistManager();
        manager
            .rebuild_track_index(&self.playlist.id, &snapshot)
            .await;
        manager.notify_playlist_changed(&self.playlist.id);

        Ok(())
    }

    /// Vide la playlist
    pub async fn flush(&self) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        let mut core = self.playlist.core.write().await;
        core.clear();
        let snapshot = core.snapshot();
        drop(core);

        self.playlist.touch().await;

        if self.playlist.persistent {
            self.save_to_db().await?;
        }

        let manager = crate::manager::PlaylistManager();
        manager
            .rebuild_track_index(&self.playlist.id, &snapshot)
            .await;
        manager.notify_playlist_changed(&self.playlist.id);

        Ok(())
    }

    /// Supprime un morceau par sa cache_pk. Retourne true si un élément a été retiré.
    pub async fn remove_track(&self, cache_pk: &str) -> Result<bool> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        let mut core = self.playlist.core.write().await;
        let removed = core.remove_by_cache_pk(cache_pk);
        let snapshot = core.snapshot();
        drop(core);

        if removed {
            self.playlist.touch().await;
            if self.playlist.persistent {
                self.save_to_db().await?;
            }

            let manager = crate::manager::PlaylistManager();
            manager
                .rebuild_track_index(&self.playlist.id, &snapshot)
                .await;
            manager.notify_playlist_changed(&self.playlist.id);
        }

        Ok(removed)
    }

    /// Met à jour la capacité maximale.
    pub async fn set_capacity(&self, max_size: Option<usize>) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        let mut core = self.playlist.core.write().await;
        core.set_capacity(max_size);
        let snapshot = core.snapshot();
        drop(core);

        self.playlist.touch().await;

        if self.playlist.persistent {
            self.save_to_db().await?;
        }

        let manager = crate::manager::PlaylistManager();
        manager
            .rebuild_track_index(&self.playlist.id, &snapshot)
            .await;
        manager.notify_playlist_changed(&self.playlist.id);

        Ok(())
    }

    /// Met à jour le TTL par défaut.
    pub async fn set_default_ttl(&self, ttl: Option<Duration>) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        let mut core = self.playlist.core.write().await;
        core.set_default_ttl(ttl);
        let snapshot = core.snapshot();
        drop(core);

        self.playlist.touch().await;

        if self.playlist.persistent {
            self.save_to_db().await?;
        }

        let manager = crate::manager::PlaylistManager();
        manager
            .rebuild_track_index(&self.playlist.id, &snapshot)
            .await;
        manager.notify_playlist_changed(&self.playlist.id);

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
        // Nettoyer les index puis supprimer du manager
        let manager = crate::manager::PlaylistManager();
        manager.rebuild_track_index(&self.playlist.id, &[]).await;
        crate::manager::delete_playlist_internal(&self.playlist.id).await?;

        // Notifier la suppression
        manager.notify_playlist_changed(&self.playlist.id);

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

        crate::manager::PlaylistManager().notify_playlist_changed(&self.playlist.id);

        Ok(())
    }

    /// Modifie le rôle logique de la playlist
    pub async fn set_role(&self, role: PlaylistRole) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        self.playlist.set_role(role).await;

        if self.playlist.persistent {
            self.save_to_db().await?;
        }

        crate::manager::PlaylistManager().notify_playlist_changed(&self.playlist.id);

        Ok(())
    }

    /// Vérifie si la playlist contient déjà un pk
    pub async fn contains_pk(&self, cache_pk: &str) -> Result<bool> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        let core = self.playlist.core.read().await;
        Ok(core.tracks.iter().any(|record| record.cache_pk == cache_pk))
    }

    /// Clone vers une nouvelle playlist persistante
    pub async fn clone_as_persistent(&self, new_id: String) -> Result<WriteHandle> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        // Récupérer les données actuelles
        let title = self.playlist.title().await;
        let role = self.playlist.role().await;
        let core = self.playlist.core.read().await;
        let config = core.config.clone();
        let tracks = core.snapshot();
        drop(core);

        // Créer la nouvelle playlist persistante
        let manager = crate::manager::PlaylistManager();
        let new_handle = manager
            .create_persistent_playlist_with_role(new_id, role)
            .await?;

        // Copier le titre et la config
        new_handle.set_title(title).await?;
        new_handle.set_capacity(config.max_size).await?;
        new_handle.set_default_ttl(config.default_ttl).await?;

        // Copier tous les morceaux
        let pks: Vec<String> = tracks.iter().map(|r| r.cache_pk.clone()).collect();
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

    pub async fn role(&self) -> PlaylistRole {
        self.playlist.role().await
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

    // ============================================================================
    // LAZY PK SUPPORT
    // ============================================================================

    /// Ajoute un track sans valider l'existence du fichier
    ///
    /// À utiliser pour les lazy PK qui seront téléchargés on-demand.
    /// Contrairement à `push()`, cette méthode ne vérifie pas si le fichier
    /// existe dans le cache avant de l'ajouter.
    ///
    /// # Arguments
    ///
    /// * `cache_pk` - PK du fichier (peut être un lazy PK "L:...")
    pub async fn push_lazy(&self, cache_pk: String) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        // PAS de validation is_valid_pk()
        let record = Record::new(cache_pk);
        let mut core = self.playlist.core.write().await;
        core.push(record);
        let snapshot = core.snapshot();
        drop(core);

        self.playlist.touch().await;

        if self.playlist.persistent {
            self.save_to_db().await?;
        }

        let manager = crate::manager::PlaylistManager();
        manager
            .rebuild_track_index(&self.playlist.id, &snapshot)
            .await;
        manager.notify_playlist_changed(&self.playlist.id);

        Ok(())
    }

    /// Version batch pour ajouter plusieurs lazy PK
    ///
    /// Plus efficace que push_lazy() en boucle car ne reconstruit
    /// l'index qu'une seule fois à la fin.
    ///
    /// # Arguments
    ///
    /// * `cache_pks` - Liste de PKs à ajouter (peuvent être des lazy PK)
    pub async fn push_lazy_batch(&self, cache_pks: Vec<String>) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        let records: Vec<Record> = cache_pks.into_iter().map(Record::new).collect();

        let mut core = self.playlist.core.write().await;
        core.push_all(records);
        let snapshot = core.snapshot();
        drop(core);

        self.playlist.touch().await;

        if self.playlist.persistent {
            self.save_to_db().await?;
        }

        let manager = crate::manager::PlaylistManager();
        manager
            .rebuild_track_index(&self.playlist.id, &snapshot)
            .await;
        manager.notify_playlist_changed(&self.playlist.id);

        Ok(())
    }

    /// Commute un cache_pk vers un nouveau PK
    ///
    /// Utilisé quand un lazy PK est téléchargé et devient un real PK.
    /// Met à jour tous les records qui utilisent l'ancien PK.
    ///
    /// # Arguments
    ///
    /// * `old_pk` - L'ancien PK (typiquement un lazy PK "L:...")
    /// * `new_pk` - Le nouveau PK (real pk calculé après téléchargement)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// // Appelé quand un lazy PK est téléchargé
    /// writer.update_cache_pk("L:abc123", "xyz789").await?;
    /// ```
    pub async fn update_cache_pk(&self, old_pk: &str, new_pk: &str) -> Result<()> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        let mut core = self.playlist.core.write().await;
        let mut updated = false;

        // Parcourir tous les records et recréer ceux qui correspondent
        // Les records sont dans des Arc, donc on doit les recréer pour les modifier
        for i in 0..core.tracks.len() {
            if let Some(record_arc) = core.tracks.get(i) {
                if record_arc.cache_pk == old_pk {
                    // Créer un nouveau record avec le nouveau PK
                    let mut new_record = (**record_arc).clone();
                    new_record.cache_pk = new_pk.to_string();
                    core.tracks[i] = Arc::new(new_record);
                    updated = true;
                }
            }
        }

        let snapshot = core.snapshot();
        drop(core);

        if updated {
            tracing::debug!(
                "Updated {} -> {} in playlist {}",
                old_pk,
                new_pk,
                self.playlist.id
            );

            self.playlist.touch().await;

            if self.playlist.persistent {
                self.save_to_db().await?;
            }

            let manager = crate::manager::PlaylistManager();
            manager
                .rebuild_track_index(&self.playlist.id, &snapshot)
                .await;
            manager.notify_playlist_changed(&self.playlist.id);
        }

        Ok(())
    }

    // Helpers internes

    async fn save_to_db(&self) -> Result<()> {
        let manager = crate::manager::PlaylistManager();
        let persistence = manager
            .persistence()
            .ok_or_else(|| crate::Error::PersistenceError("No persistence manager".into()))?;

        let title = self.playlist.title().await;
        let role = self.playlist.role().await;
        let core = self.playlist.core.read().await;
        let config = &core.config;
        let tracks = &core.tracks;

        persistence
            .save_playlist(&self.playlist.id, &title, &role, config, tracks)
            .await
    }
}
