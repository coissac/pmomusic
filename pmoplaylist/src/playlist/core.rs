//! PlaylistCore : structure FIFO avec éviction automatique

use super::record::Record;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

/// Configuration d'une playlist
#[derive(Debug, Clone)]
pub struct PlaylistConfig {
    pub max_size: Option<usize>,
    pub default_ttl: Option<Duration>,
}

impl Default for PlaylistConfig {
    fn default() -> Self {
        Self {
            max_size: None,
            default_ttl: None,
        }
    }
}

/// Noyau de la playlist (structure interne protégée par RwLock)
pub struct PlaylistCore {
    pub tracks: VecDeque<Arc<Record>>,
    pub config: PlaylistConfig,
}

impl PlaylistCore {
    /// Crée un nouveau core
    pub fn new(config: PlaylistConfig) -> Self {
        Self {
            tracks: VecDeque::new(),
            config,
        }
    }

    /// Ajoute un record et applique l'éviction
    pub fn push(&mut self, record: Record) {
        self.tracks.push_back(Arc::new(record));
        self.evict();
    }

    /// Ajoute plusieurs records de manière atomique
    pub fn push_all(&mut self, records: Vec<Record>) {
        for record in records {
            self.tracks.push_back(Arc::new(record));
        }
        self.evict();
    }

    /// Nettoie les morceaux expirés et applique la limite de taille
    pub fn evict(&mut self) {
        // 1. Supprimer les morceaux périmés par TTL
        self.tracks
            .retain(|record| !record.is_expired(self.config.default_ttl));

        // 2. Appliquer la limite de taille (FIFO)
        if let Some(max) = self.config.max_size {
            while self.tracks.len() > max {
                self.tracks.pop_front();
            }
        }
    }

    /// Vide complètement la playlist
    pub fn clear(&mut self) {
        self.tracks.clear();
    }

    /// Nombre de morceaux
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// Vérifie si la playlist est vide
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Récupère un record par index
    pub fn get(&self, index: usize) -> Option<Arc<Record>> {
        self.tracks.get(index).cloned()
    }

    /// Snapshot de tous les records
    pub fn snapshot(&self) -> Vec<Arc<Record>> {
        self.tracks.iter().cloned().collect()
    }

    /// Supprime un record par cache_pk (retourne true si supprimé)
    pub fn remove_by_cache_pk(&mut self, cache_pk: &str) -> bool {
        let initial_len = self.tracks.len();
        self.tracks.retain(|r| r.cache_pk != cache_pk);
        self.tracks.len() != initial_len
    }

    /// Met à jour la capacité maximale
    pub fn set_capacity(&mut self, max_size: Option<usize>) {
        self.config.max_size = max_size;
        self.evict();
    }

    /// Met à jour le TTL par défaut
    pub fn set_default_ttl(&mut self, ttl: Option<Duration>) {
        self.config.default_ttl = ttl;
        self.evict();
    }
}
