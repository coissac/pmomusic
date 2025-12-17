//! PlaylistManager : gestionnaire singleton central de toutes les playlists

use crate::handle::{ReadHandle, WriteHandle};
use crate::persistence::PersistenceManager;
use crate::playlist::core::PlaylistConfig;
use crate::playlist::Playlist;
use crate::Result;
use once_cell::sync::OnceCell;
use pmocache::{CacheBroadcastEvent, CacheEvent, CacheSubscription};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock as StdRwLock;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::RwLock;

/// Singleton PlaylistManager
static PLAYLIST_MANAGER: OnceCell<PlaylistManager> = OnceCell::new();

/// Registre global du cache audio
static AUDIO_CACHE: OnceCell<Arc<pmoaudiocache::Cache>> = OnceCell::new();

/// Structure interne du manager
struct ManagerInner {
    playlists: RwLock<HashMap<String, Arc<Playlist>>>,
    persistence: Option<Arc<PersistenceManager>>,
    callbacks: StdRwLock<HashMap<u64, Arc<dyn Fn(&PlaylistEvent) + Send + Sync>>>,
    cb_counter: AtomicU64,
    track_index: StdRwLock<HashMap<String, Vec<String>>>, // cache_pk -> playlists
    cache_subscriptions: StdRwLock<HashMap<String, CacheSubscription>>,
    event_tx: broadcast::Sender<PlaylistEventEnvelope>,
    lazy_listener_started: AtomicBool,
}

/// Type d'évènement émis par le PlaylistManager.
#[derive(Debug, Clone)]
pub struct PlaylistEvent {
    pub playlist_id: String,
    pub kind: PlaylistEventKind,
}

/// Variantes d'évènements playlist.
#[derive(Debug, Clone)]
pub enum PlaylistEventKind {
    /// La playlist a été modifiée (ajout/suppression/changement de config).
    Updated,
    /// Un morceau référencé par la playlist a été servi par le cache audio.
    TrackPlayed { cache_pk: String, qualifier: String },
}

/// Evènement enrichi pour diffusion (timestamp + source client éventuel).
#[derive(Debug, Clone)]
pub struct PlaylistEventEnvelope {
    pub event: PlaylistEvent,
    pub timestamp: std::time::SystemTime,
    pub source_client: Option<String>,
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

        // Lancer la consolidation en arrière-plan
        let persistence_clone = persistence.clone();
        tokio::spawn(async move {
            if let Err(e) = persistence_clone.consolidate().await {
                tracing::warn!("Failed to consolidate playlist database on startup: {}", e);
            }
        });

        let manager = Self {
            inner: Arc::new(ManagerInner {
                playlists: RwLock::new(HashMap::new()),
                persistence: Some(persistence.clone()),
                callbacks: StdRwLock::new(HashMap::new()),
                cb_counter: AtomicU64::new(1),
                track_index: StdRwLock::new(HashMap::new()),
                cache_subscriptions: StdRwLock::new(HashMap::new()),
                event_tx: broadcast::channel(256).0,
                lazy_listener_started: AtomicBool::new(false),
            }),
        };

        // Lancer la task d'�viction en background
        {
            let manager_clone = manager.clone();
            tokio::spawn(async move {
                manager_clone.eviction_task().await;
            });
        }

        {
            let manager_clone = manager.clone();
            tokio::spawn(async move {
                manager_clone.ensure_lazy_listener().await;
            });
        }

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
            PLAYLIST_MANAGER
                .get()
                .expect("PlaylistManager not initialized. Call init() first.")
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
            persistence
                .save_playlist(&playlist.id, &title, &core.config, &core.tracks)
                .await?;
        }

        Ok(WriteHandle::new(playlist, write_token))
    }

    /// Enregistre un callback d'évènement playlist (update, track joué).
    ///
    /// Retourne un jeton (u64) pour désenregistrer plus tard.
    pub fn register_callback<F>(&self, cb: F) -> u64
    where
        F: Fn(&PlaylistEvent) + Send + Sync + 'static,
    {
        let token = self.inner.cb_counter.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.inner.callbacks.write().unwrap();
        guard.insert(token, Arc::new(cb));
        token
    }

    /// Désenregistre un callback via son jeton.
    pub fn unregister_callback(&self, token: u64) {
        let mut guard = self.inner.callbacks.write().unwrap();
        guard.remove(&token);
    }

    /// Notifie tous les callbacks qu'une playlist a changé.
    pub(crate) fn notify_playlist_changed(&self, id: &str) {
        self.notify_playlist_event(id, PlaylistEventKind::Updated);
    }

    /// Notifie les callbacks qu'un morceau a été joué pour une playlist donnée.
    pub(crate) fn notify_playlist_track_played(
        &self,
        playlist_id: &str,
        cache_pk: &str,
        qualifier: &str,
    ) {
        self.notify_playlist_event(
            playlist_id,
            PlaylistEventKind::TrackPlayed {
                cache_pk: cache_pk.to_string(),
                qualifier: qualifier.to_string(),
            },
        );
    }

    fn notify_playlist_event(&self, id: &str, kind: PlaylistEventKind) {
        let event = PlaylistEvent {
            playlist_id: id.to_string(),
            kind,
        };
        let envelope = PlaylistEventEnvelope {
            event: event.clone(),
            timestamp: std::time::SystemTime::now(),
            source_client: None,
        };

        let guard = self.inner.callbacks.read().unwrap();
        for cb in guard.values() {
            cb(&event);
        }

        // Diffusion via canal interne (ignoré si aucun abonné)
        let _ = self.inner.event_tx.send(envelope);
    }

    /// Ré-inscrit les abonnements cache pour tous les pk connus (utilisé au boot ou après enregistrement du cache audio).
    async fn sync_cache_subscriptions(&self) {
        let pks: Vec<String> = {
            let index = self.inner.track_index.read().unwrap();
            index.keys().cloned().collect()
        };

        if pks.is_empty() {
            return;
        }

        if let Ok(cache) = audio_cache() {
            for pk in pks {
                // Ne pas doubler les abonnements
                let already = {
                    let subs = self.inner.cache_subscriptions.read().unwrap();
                    subs.contains_key(&pk)
                };
                if already {
                    continue;
                }

                let manager = self.clone();
                let token = cache
                    .subscribe_broadcast(pk.clone(), move |event: &CacheBroadcastEvent| {
                        manager.handle_cache_broadcast(event);
                        let index = manager.inner.track_index.read().unwrap();
                        index.contains_key(&event.pk)
                    })
                    .await;

                self.inner
                    .cache_subscriptions
                    .write()
                    .unwrap()
                    .insert(pk, token);
            }
        }
    }

    /// Réconcilie l'index pk→playlists et les souscriptions cache pour une playlist donnée.
    pub(crate) async fn rebuild_track_index(
        &self,
        playlist_id: &str,
        records: &[Arc<crate::playlist::record::Record>],
    ) {
        // 1) Retirer la playlist de toutes les entrées
        let mut removed_pks = Vec::new();
        {
            let mut index = self.inner.track_index.write().unwrap();
            for (pk, playlists) in index.iter_mut() {
                playlists.retain(|p| p != playlist_id);
                if playlists.is_empty() {
                    removed_pks.push(pk.clone());
                }
            }
            for pk in &removed_pks {
                index.remove(pk);
            }
            // 2) Ajouter les nouveaux records
            for record in records {
                let entry = index.entry(record.cache_pk.clone()).or_default();
                if !entry.iter().any(|p| p == playlist_id) {
                    entry.push(playlist_id.to_string());
                }
            }
        }

        // 3) Se désabonner des pk qui ne sont plus référencés
        // Collecter les tokens à désinscrire sans bloquer pendant l'await
        let removed_tokens: Vec<CacheSubscription> = {
            if removed_pks.is_empty() {
                Vec::new()
            } else {
                let mut subs = self.inner.cache_subscriptions.write().unwrap();
                removed_pks
                    .into_iter()
                    .filter_map(|pk| subs.remove(&pk))
                    .collect()
            }
        };

        if !removed_tokens.is_empty() {
            if let Ok(cache) = audio_cache() {
                for token in removed_tokens {
                    cache.unsubscribe_broadcast(&token).await;
                }
            }
        }

        // 4) S'abonner aux nouveaux pk sans souscription
        let missing: Vec<String> = {
            let index = self.inner.track_index.read().unwrap();
            let subs = self.inner.cache_subscriptions.read().unwrap();
            index
                .iter()
                .filter_map(|(pk, playlists)| {
                    if playlists.contains(&playlist_id.to_string()) && !subs.contains_key(pk) {
                        Some(pk.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        if !missing.is_empty() {
            if let Ok(cache) = audio_cache() {
                for pk in missing {
                    let manager = self.clone();
                    let token = cache
                        .subscribe_broadcast(pk.clone(), move |event: &CacheBroadcastEvent| {
                            manager.handle_cache_broadcast(event);
                            // Garder l'abonnement tant que le pk est référencé
                            let index = manager.inner.track_index.read().unwrap();
                            index.contains_key(&event.pk)
                        })
                        .await;
                    self.inner
                        .cache_subscriptions
                        .write()
                        .unwrap()
                        .insert(pk, token);
                }
            }
        }
    }

    fn handle_cache_broadcast(&self, event: &CacheBroadcastEvent) {
        let playlists = {
            let index = self.inner.track_index.read().unwrap();
            index.get(&event.pk).cloned()
        };

        if let Some(playlists) = playlists {
            for playlist_id in playlists {
                self.notify_playlist_track_played(&playlist_id, &event.pk, &event.qualifier);
            }
        }
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
            // Playlist existe en mémoire
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

        // Pas en mémoire, essayer de charger depuis la DB
        if let Some(persistence) = &self.inner.persistence {
            if let Some((title, config, tracks)) = persistence.load_playlist(&id).await? {
                // Reconstruire la playlist
                let mut playlists = self.inner.playlists.write().await;

                let playlist = Arc::new(Playlist::new(id.clone(), title.clone(), config, true));

                // Restaurer les tracks
                {
                    let mut core = playlist.core.write().await;
                    core.tracks = tracks;
                    let snapshot = core.snapshot();
                    drop(core);
                    self.rebuild_track_index(&id, &snapshot).await;
                }

                // Acquérir le write lock
                let write_token = playlist
                    .acquire_write_lock()
                    .await
                    .map_err(|_| crate::Error::WriteLockHeld(id.clone()))?;

                playlists.insert(id.clone(), playlist.clone());
                drop(playlists);

                return Ok(WriteHandle::new(playlist, write_token));
            }
        }

        // N'existe pas en DB, créer une nouvelle playlist persistante
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

                let playlist = Arc::new(Playlist::new(id.to_string(), title.clone(), config, true));

                // Restaurer les tracks
                {
                    let mut core = playlist.core.write().await;
                    core.tracks = tracks;
                    let snapshot = core.snapshot();
                    drop(core);
                    self.rebuild_track_index(id, &snapshot).await;
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

        // Nettoyer l'index et les souscriptions
        self.rebuild_track_index(id, &[]).await;

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

    // ============================================================================
    // LAZY PK SUPPORT
    // ============================================================================

    /// Active le mode lazy pour une playlist
    ///
    /// Configure l'écoute des events du cache pour :
    /// 1. Commuter automatiquement les lazy PK vers real PK après téléchargement
    /// 2. Prefetch intelligent des N tracks suivants
    ///
    /// # Arguments
    ///
    /// * `playlist_id` - ID de la playlist à gérer
    /// * `lookahead` - Nombre de tracks à prefetch (recommandé: 3-5)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let manager = PlaylistManager::get();
    /// manager.enable_lazy_mode("qobuz-favorites-123", 5);
    /// // → La playlist commute automatiquement lazy → real PK
    /// // → Prefetch 5 tracks en avance pendant la lecture
    /// ```
    pub fn enable_lazy_mode(&self, playlist_id: &str, lookahead: usize) {
        let playlist_id = playlist_id.to_string();

        // Obtenir le cache audio
        let cache = match audio_cache() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Cannot enable lazy mode: audio cache not available: {}", e);
                return;
            }
        };

        // S'abonner aux events du cache
        let mut rx = cache.subscribe_events();
        let manager = self.clone();

        tokio::spawn(async move {
            tracing::info!(
                "Lazy mode enabled for playlist {} (lookahead: {})",
                playlist_id,
                lookahead
            );

            while let Ok(event) = rx.recv().await {
                match event {
                    pmocache::CacheEvent::LazyDownloaded { lazy_pk, real_pk } => {
                        tracing::debug!("Received LazyDownloaded event: {} → {}", lazy_pk, real_pk);

                        // 1. Commuter le PK dans la playlist
                        if let Ok(writer) = manager.get_write_handle(playlist_id.clone()).await {
                            tracing::info!(
                                "Switching PK in playlist {}: {} -> {}",
                                playlist_id,
                                lazy_pk,
                                real_pk
                            );
                            if let Err(e) = writer.update_cache_pk(&lazy_pk, &real_pk).await {
                                tracing::error!("Failed to update PK in playlist: {}", e);
                            }
                        }

                        // 2. Prefetch les tracks suivants
                        manager
                            .prefetch_next_tracks(&playlist_id, &real_pk, lookahead)
                            .await;
                    }
                    _ => {}
                }
            }

            tracing::warn!("Lazy mode listener stopped for playlist {}", playlist_id);
        });
    }

    /// Prefetch les N tracks suivants après une position donnée
    ///
    /// Cette méthode est appelée automatiquement par `enable_lazy_mode()`.
    async fn prefetch_next_tracks(&self, playlist_id: &str, current_pk: &str, lookahead: usize) {
        let playlist = {
            let playlists = self.inner.playlists.read().await;
            playlists.get(playlist_id).cloned()
        };

        let Some(playlist) = playlist else {
            return;
        };

        let core = playlist.core.read().await;
        let tracks = core.snapshot();

        // Trouver position actuelle
        let Some(pos) = tracks.iter().position(|r| &r.cache_pk == current_pk) else {
            return;
        };

        // Prefetch N tracks suivants
        let cache = match audio_cache() {
            Ok(c) => c,
            Err(_) => return,
        };

        for i in (pos + 1)..=(pos + lookahead).min(tracks.len() - 1) {
            let next_pk = &tracks[i].cache_pk;

            // Si lazy PK, déclencher download en background
            if pmocache::is_lazy_pk(next_pk) {
                tracing::debug!("Prefetching lazy track {}: {}", i, next_pk);

                let cache = cache.clone();
                let next_pk = next_pk.clone();

                tokio::spawn(async move {
                    // Récupérer l'origin_url depuis la DB
                    let origin_url = match cache.db.get_origin_url(&next_pk) {
                        Ok(Some(url)) => url,
                        Ok(None) => {
                            tracing::warn!("No origin_url for lazy pk {}", next_pk);
                            return;
                        }
                        Err(e) => {
                            tracing::error!("Error getting origin_url for {}: {}", next_pk, e);
                            return;
                        }
                    };

                    // Déclencher download (ne bloque pas)
                    if let Err(e) = cache.add_from_url(&origin_url, None).await {
                        tracing::error!("Failed to prefetch {}: {}", next_pk, e);
                    } else {
                        tracing::debug!("Prefetch completed for {}", next_pk);
                    }
                });
            }
        }
    }

    async fn ensure_lazy_listener(&self) {
        if self.inner.lazy_listener_started.load(Ordering::SeqCst) {
            return;
        }

        let cache = match audio_cache() {
            Ok(cache) => cache,
            Err(_) => return,
        };

        if self
            .inner
            .lazy_listener_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let manager = self.clone();
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let mut rx = cache.subscribe_events();
            while let Ok(event) = rx.recv().await {
                if let CacheEvent::LazyDownloaded { lazy_pk, real_pk } = event {
                    manager
                        .handle_lazy_download_event(&lazy_pk, &real_pk)
                        .await;
                }
            }
            inner.lazy_listener_started.store(false, Ordering::SeqCst);
        });
    }

    async fn handle_lazy_download_event(&self, lazy_pk: &str, real_pk: &str) {
        let playlists = {
            let index = self.inner.track_index.read().unwrap();
            index.get(lazy_pk).cloned()
        };

        let Some(playlists) = playlists else {
            tracing::debug!(
                "Lazy download {} converted to {} but no playlists referenced it",
                lazy_pk,
                real_pk
            );
            return;
        };

        for playlist_id in playlists {
            match self.get_write_handle(playlist_id.clone()).await {
                Ok(writer) => {
                    if let Err(e) = writer.update_cache_pk(lazy_pk, real_pk).await {
                        tracing::error!(
                            "Failed to update playlist {} from {} to {}: {}",
                            playlist_id,
                            lazy_pk,
                            real_pk,
                            e
                        );
                    }
                }
                Err(e) => tracing::debug!(
                    "Failed to acquire write handle for playlist {} during lazy swap: {}",
                    playlist_id,
                    e
                ),
            }
        }
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
                        let _ = persistence
                            .save_playlist(&playlist.id, &title, &core.config, &core.tracks)
                            .await;
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

/// Souscrit au flux d'évènements playlist (Updated / TrackPlayed) avec timestamp.
pub fn subscribe_events() -> broadcast::Receiver<PlaylistEventEnvelope> {
    PlaylistManager::get().inner.event_tx.subscribe()
}

/// Helper pour supprimer une playlist (appel� depuis WriteHandle)
pub(crate) async fn delete_playlist_internal(id: &str) -> Result<()> {
    PlaylistManager::get().delete_playlist(id).await
}

/// Enregistre le cache audio global
///
/// Cette fonction doit être appelée au démarrage de l'application
/// pour rendre le cache audio disponible au PlaylistManager.
///
/// # Examples
///
/// ```rust,ignore
/// use pmoplaylist::register_audio_cache;
/// use pmoaudiocache::Cache as AudioCache;
/// use std::sync::Arc;
///
/// let audio_cache = Arc::new(AudioCache::new("./cache", 1000)?);
/// register_audio_cache(audio_cache);
/// ```
pub fn register_audio_cache(cache: Arc<pmoaudiocache::Cache>) {
    let _ = AUDIO_CACHE.set(cache);

    // Si le PlaylistManager est déjà initialisé, synchroniser les abonnements
    if let Some(manager) = PLAYLIST_MANAGER.get() {
        let manager = manager.clone();
        tokio::spawn(async move {
            manager.sync_cache_subscriptions().await;
            manager.ensure_lazy_listener().await;
        });
    }
}

/// Helper pour acc�der au cache audio
pub(crate) fn audio_cache() -> Result<Arc<pmoaudiocache::Cache>> {
    AUDIO_CACHE
        .get()
        .cloned()
        .or_else(|| {
            // Fallback: essayer le registre global de pmoaudiocache
            pmoaudiocache::get_audio_cache()
        })
        .ok_or_else(|| crate::Error::ManagerNotInitialized)
}

/// Fonction raccourcie pour acc�der au singleton
pub fn PlaylistManager() -> &'static PlaylistManager {
    PlaylistManager::get()
}
