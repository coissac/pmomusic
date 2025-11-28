//! ReadHandle : consommation individuelle d'une playlist

use crate::playlist::Playlist;
use crate::track::PlaylistTrack;
use crate::Result;
use pmocache::cache_trait::FileCache;
use pmodidl::{Container, Item};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Handle de lecture sur une playlist (peut avoir plusieurs instances)
pub struct ReadHandle {
    playlist: Arc<Playlist>,
    cursor: AtomicUsize,
}

impl ReadHandle {
    /// Crée un nouveau handle de lecture
    pub(crate) fn new(playlist: Arc<Playlist>) -> Self {
        Self {
            playlist,
            cursor: AtomicUsize::new(0),
        }
    }

    /// Pop le prochain morceau (avance le curseur)
    ///
    /// Skip automatiquement les entrées invalides dans le cache.
    pub async fn pop(&self) -> Result<Option<PlaylistTrack>> {
        loop {
            if !self.playlist.is_alive() {
                return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
            }

            let pos = self.cursor.load(Ordering::SeqCst);

            let core = self.playlist.core.read().await;

            // Fin de playlist ?
            if pos >= core.len() {
                return Ok(None);
            }

            let record = match core.get(pos) {
                Some(r) => r,
                None => return Ok(None),
            };

            let cache_pk = record.cache_pk.clone();
            drop(core);

            // Vérifier validité dans le cache
            let cache = crate::manager::audio_cache()?;
            if cache.is_valid_pk(&cache_pk).await {
                // Valide, avancer le curseur et retourner
                self.cursor.fetch_add(1, Ordering::SeqCst);
                return Ok(Some(PlaylistTrack::new(cache_pk)));
            } else {
                // Invalide, supprimer de la playlist et continuer
                tracing::warn!("Cache entry {} missing, removing from playlist", cache_pk);
                let mut core = self.playlist.core.write().await;
                core.remove_by_cache_pk(&cache_pk);
                drop(core);

                // Sauvegarder si persistante
                if self.playlist.persistent {
                    if let Some(persistence) = crate::manager::PlaylistManager().persistence() {
                        let title = self.playlist.title().await;
                        let core = self.playlist.core.read().await;
                        let _ = persistence
                            .save_playlist(&self.playlist.id, &title, &core.config, &core.tracks)
                            .await;
                    }
                }

                // Ne pas avancer le curseur, continuer avec la position actuelle
                continue;
            }
        }
    }

    /// Peek le prochain morceau sans avancer le curseur
    pub async fn peek(&self) -> Result<Option<PlaylistTrack>> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        let pos = self.cursor.load(Ordering::SeqCst);
        let core = self.playlist.core.read().await;

        match core.get(pos) {
            Some(record) => {
                // Vérifier validité
                let cache = crate::manager::audio_cache()?;
                if cache.is_valid_pk(&record.cache_pk).await {
                    Ok(Some(PlaylistTrack::new(record.cache_pk.clone())))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Position actuelle du curseur
    pub fn position(&self) -> usize {
        self.cursor.load(Ordering::SeqCst)
    }

    /// Nombre de morceaux restants (compte uniquement les valides)
    pub async fn remaining(&self) -> Result<usize> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        let pos = self.cursor.load(Ordering::SeqCst);
        let core = self.playlist.core.read().await;

        if pos >= core.len() {
            return Ok(0);
        }

        let cache = crate::manager::audio_cache()?;
        let mut count = 0;

        for i in pos..core.len() {
            if let Some(record) = core.get(i) {
                if cache.is_valid_pk(&record.cache_pk).await {
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Crée un nouveau handle avec cursor à 0
    pub fn get_new_handle(&self) -> Result<ReadHandle> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        Ok(ReadHandle::new(self.playlist.clone()))
    }

    /// Vérifie si la playlist est vivante
    pub fn is_alive(&self) -> bool {
        self.playlist.is_alive()
    }

    /// ID de la playlist
    pub fn id(&self) -> &str {
        &self.playlist.id
    }

    /// Génère un Container DIDL-Lite
    pub async fn to_container(&self) -> Result<Container> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        let title = self.playlist.title().await;
        let remaining = self.remaining().await?;

        Ok(Container {
            id: self.playlist.id.clone(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("0".to_string()),
            title,
            class: "object.container.playlistContainer".to_string(),
            containers: vec![],
            items: vec![],
        })
    }

    /// Génère des Items DIDL-Lite depuis la position actuelle
    pub async fn to_items(&self, limit: usize) -> Result<Vec<Item>> {
        if !self.playlist.is_alive() {
            return Err(crate::Error::PlaylistDeleted(self.playlist.id.clone()));
        }

        let pos = self.cursor.load(Ordering::SeqCst);
        let core = self.playlist.core.read().await;
        let cache = crate::manager::audio_cache()?;

        // Récupérer base_url depuis le cache (via route_for)
        let mut items = Vec::new();
        let mut idx = 0;

        for i in pos..core.len() {
            if items.len() >= limit {
                break;
            }

            let record = match core.get(i) {
                Some(r) => r,
                None => continue,
            };

            // Vérifier validité
            if !cache.is_valid_pk(&record.cache_pk).await {
                continue;
            }

            // Charger métadonnées via TrackMetadata
            use pmoaudiocache::metadata_ext::{AudioTrackMetadataExt, TrackMetadataDidlExt};
            let track_meta = cache.track_metadata(&record.cache_pk);
            let meta = track_meta.read().await;

            // Construire l'URL via route_for
            let url = cache.route_for(&record.cache_pk, None);

            // Créer le Resource DIDL via l'extension trait
            let resource = meta.to_didl_resource(url).await;

            // Récupérer les métadonnées pour construire l'Item DIDL
            let title = meta.get_title().await.ok().flatten().unwrap_or_else(|| "Unknown".to_string());
            let artist = meta.get_artist().await.ok().flatten();
            let album = meta.get_album().await.ok().flatten();
            let genre = meta.get_genre().await.ok().flatten();
            let year = meta.get_year().await.ok().flatten();
            let track_number = meta.get_track_number().await.ok().flatten();
            let cover_pk = meta.get_cover_pk().await.ok().flatten();
            let cover_url = if let Some(pk) = cover_pk.as_ref() {
                Some(format!("/covers/jpeg/{}/256", pk))
            } else {
                meta.get_cover_url().await.ok().flatten()
            };

            // Créer l'Item
            let item = Item {
                id: format!("{}:{}", self.playlist.id, pos + idx),
                parent_id: self.playlist.id.clone(),
                restricted: Some("1".to_string()),
                title: title.clone(),
                creator: artist.clone(),
                class: "object.item.audioItem.musicTrack".to_string(),
                artist,
                album,
                genre,
                album_art: cover_url,
                album_art_pk: cover_pk,
                date: year.map(|y| y.to_string()),
                original_track_number: track_number.map(|n| n.to_string()),
                resources: vec![resource],
                descriptions: vec![],
            };

            items.push(item);
            idx += 1;
        }

        Ok(items)
    }
}
