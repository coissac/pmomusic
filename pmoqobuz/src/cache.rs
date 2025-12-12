//! Système de cache en mémoire pour les données Qobuz
//!
//! Ce module fournit un cache en mémoire avec TTL pour minimiser les requêtes à l'API Qobuz.

use crate::models::{Album, Artist, Playlist, SearchResult, StreamInfo, Track};
use moka::future::Cache as MokaCache;
use std::sync::Arc;
use std::time::Duration;

/// Cache principal pour les données Qobuz
#[derive(Clone)]
pub struct QobuzCache {
    /// Cache des albums (TTL: 1 heure)
    albums: Arc<MokaCache<String, Album>>,
    /// Cache des tracks (TTL: 1 heure)
    tracks: Arc<MokaCache<String, Track>>,
    /// Cache des artistes (TTL: 1 heure)
    artists: Arc<MokaCache<String, Artist>>,
    /// Cache des playlists (TTL: 30 minutes)
    playlists: Arc<MokaCache<String, Playlist>>,
    /// Cache des résultats de recherche (TTL: 15 minutes)
    searches: Arc<MokaCache<String, SearchResult>>,
    /// Cache des URLs de streaming (TTL: 5 minutes)
    stream_urls: Arc<MokaCache<String, StreamInfo>>,
}

impl QobuzCache {
    /// Crée un nouveau cache avec les paramètres par défaut
    pub fn new() -> Self {
        Self::with_capacity(1000)
    }

    /// Crée un nouveau cache avec une capacité spécifique
    pub fn with_capacity(max_capacity: u64) -> Self {
        Self {
            albums: Arc::new(
                MokaCache::builder()
                    .max_capacity(max_capacity)
                    .time_to_live(Duration::from_secs(3600)) // 1 heure
                    .build(),
            ),
            tracks: Arc::new(
                MokaCache::builder()
                    .max_capacity(max_capacity * 2)
                    .time_to_live(Duration::from_secs(3600)) // 1 heure
                    .build(),
            ),
            artists: Arc::new(
                MokaCache::builder()
                    .max_capacity(max_capacity / 2)
                    .time_to_live(Duration::from_secs(3600)) // 1 heure
                    .build(),
            ),
            playlists: Arc::new(
                MokaCache::builder()
                    .max_capacity(max_capacity / 4)
                    .time_to_live(Duration::from_secs(1800)) // 30 minutes
                    .build(),
            ),
            searches: Arc::new(
                MokaCache::builder()
                    .max_capacity(max_capacity / 2)
                    .time_to_live(Duration::from_secs(900)) // 15 minutes
                    .build(),
            ),
            stream_urls: Arc::new(
                MokaCache::builder()
                    .max_capacity(max_capacity / 4)
                    .time_to_live(Duration::from_secs(300)) // 5 minutes
                    .build(),
            ),
        }
    }

    // ============ Albums ============

    /// Récupère un album depuis le cache
    pub async fn get_album(&self, id: &str) -> Option<Album> {
        self.albums.get(id).await
    }

    /// Ajoute un album au cache
    pub async fn put_album(&self, id: String, album: Album) {
        self.albums.insert(id, album).await;
    }

    /// Invalide un album du cache
    pub async fn invalidate_album(&self, id: &str) {
        self.albums.invalidate(id).await;
    }

    // ============ Tracks ============

    /// Récupère une track depuis le cache
    pub async fn get_track(&self, id: &str) -> Option<Track> {
        self.tracks.get(id).await
    }

    /// Ajoute une track au cache
    pub async fn put_track(&self, id: String, track: Track) {
        self.tracks.insert(id, track).await;
    }

    /// Invalide une track du cache
    pub async fn invalidate_track(&self, id: &str) {
        self.tracks.invalidate(id).await;
    }

    // ============ Artists ============

    /// Récupère un artiste depuis le cache
    pub async fn get_artist(&self, id: &str) -> Option<Artist> {
        self.artists.get(id).await
    }

    /// Ajoute un artiste au cache
    pub async fn put_artist(&self, id: String, artist: Artist) {
        self.artists.insert(id, artist).await;
    }

    /// Invalide un artiste du cache
    pub async fn invalidate_artist(&self, id: &str) {
        self.artists.invalidate(id).await;
    }

    // ============ Playlists ============

    /// Récupère une playlist depuis le cache
    pub async fn get_playlist(&self, id: &str) -> Option<Playlist> {
        self.playlists.get(id).await
    }

    /// Ajoute une playlist au cache
    pub async fn put_playlist(&self, id: String, playlist: Playlist) {
        self.playlists.insert(id, playlist).await;
    }

    /// Invalide une playlist du cache
    pub async fn invalidate_playlist(&self, id: &str) {
        self.playlists.invalidate(id).await;
    }

    // ============ Recherches ============

    /// Récupère un résultat de recherche depuis le cache
    pub async fn get_search(&self, query: &str) -> Option<SearchResult> {
        self.searches.get(query).await
    }

    /// Ajoute un résultat de recherche au cache
    pub async fn put_search(&self, query: String, result: SearchResult) {
        self.searches.insert(query, result).await;
    }

    /// Invalide un résultat de recherche du cache
    pub async fn invalidate_search(&self, query: &str) {
        self.searches.invalidate(query).await;
    }

    // ============ URLs de streaming ============

    /// Récupère une URL de streaming depuis le cache
    pub async fn get_stream_url(&self, track_id: &str) -> Option<StreamInfo> {
        self.stream_urls.get(track_id).await
    }

    /// Ajoute une URL de streaming au cache
    pub async fn put_stream_url(&self, track_id: String, info: StreamInfo) {
        self.stream_urls.insert(track_id, info).await;
    }

    /// Invalide une URL de streaming du cache
    pub async fn invalidate_stream_url(&self, track_id: &str) {
        self.stream_urls.invalidate(track_id).await;
    }

    // ============ Maintenance ============

    /// Vide tous les caches
    pub async fn clear_all(&self) {
        self.albums.invalidate_all();
        self.tracks.invalidate_all();
        self.artists.invalidate_all();
        self.playlists.invalidate_all();
        self.searches.invalidate_all();
        self.stream_urls.invalidate_all();
    }

    /// Retourne des statistiques sur le cache
    pub async fn stats(&self) -> CacheStats {
        self.albums.run_pending_tasks().await;
        self.tracks.run_pending_tasks().await;
        self.artists.run_pending_tasks().await;
        self.playlists.run_pending_tasks().await;
        self.searches.run_pending_tasks().await;
        self.stream_urls.run_pending_tasks().await;

        CacheStats {
            albums_count: self.albums.entry_count(),
            tracks_count: self.tracks.entry_count(),
            artists_count: self.artists.entry_count(),
            playlists_count: self.playlists.entry_count(),
            searches_count: self.searches.entry_count(),
            stream_urls_count: self.stream_urls.entry_count(),
        }
    }
}

impl Default for QobuzCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistiques du cache
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheStats {
    /// Nombre d'albums en cache
    pub albums_count: u64,
    /// Nombre de tracks en cache
    pub tracks_count: u64,
    /// Nombre d'artistes en cache
    pub artists_count: u64,
    /// Nombre de playlists en cache
    pub playlists_count: u64,
    /// Nombre de recherches en cache
    pub searches_count: u64,
    /// Nombre d'URLs de streaming en cache
    pub stream_urls_count: u64,
}

impl CacheStats {
    /// Retourne le nombre total d'entrées en cache
    pub fn total_count(&self) -> u64 {
        self.albums_count
            + self.tracks_count
            + self.artists_count
            + self.playlists_count
            + self.searches_count
            + self.stream_urls_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Artist;

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache = QobuzCache::new();

        let artist = Artist::new("123", "Test Artist");

        // Test insertion
        cache.put_artist("123".to_string(), artist.clone()).await;

        // Test récupération
        let retrieved = cache.get_artist("123").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Artist");

        // Test invalidation
        cache.invalidate_artist("123").await;
        let after_invalidation = cache.get_artist("123").await;
        assert!(after_invalidation.is_none());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = QobuzCache::new();

        let artist = Artist::new("123", "Test Artist");
        cache.put_artist("123".to_string(), artist).await;

        let stats = cache.stats().await;
        assert_eq!(stats.artists_count, 1);
        assert_eq!(stats.albums_count, 0);
    }

    #[tokio::test]
    async fn test_cache_clear_all() {
        let cache = QobuzCache::new();

        let artist = Artist::new("123", "Test Artist");
        cache.put_artist("123".to_string(), artist).await;

        cache.clear_all().await;

        let stats = cache.stats().await;
        assert_eq!(stats.total_count(), 0);
    }
}
