//! Client principal pour interagir avec l'API Qobuz
//!
//! Ce module fournit un client haut-niveau avec authentification et cache intégré.

use crate::api::auth::AuthInfo;
use crate::api::{QobuzApi, DEFAULT_APP_ID};
use crate::cache::QobuzCache;
use crate::config_ext::QobuzConfigExt;
use crate::error::{QobuzError, Result};
use crate::models::*;
use pmoconfig::{self, Config};
use std::future::Future;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

/// Client Qobuz haut-niveau avec cache
pub struct QobuzClient {
    /// API bas-niveau
    api: QobuzApi,
    /// Cache en mémoire
    cache: Arc<QobuzCache>,
    /// Informations d'authentification
    auth_info: Mutex<Option<AuthInfo>>,
    /// Identifiants utilisateur pour relogin automatique
    credentials: Option<(String, String)>,
    /// Configuration partagée (pour persister les tokens)
    config: Option<Arc<Config>>,
    #[cfg(feature = "disk-cache")]
    /// Cache disque optionnel
    disk_cache: Option<Arc<dyn crate::disk_cache::CacheStore>>,
}

impl QobuzClient {
    fn build_client(
        api: QobuzApi,
        auth_info: Option<AuthInfo>,
        credentials: Option<(String, String)>,
        config: Option<Arc<Config>>,
    ) -> Self {
        if let Some(info) = &auth_info {
            api.set_auth_token(info.token.clone(), info.user_id.clone());
        }

        Self {
            api,
            cache: Arc::new(QobuzCache::new()),
            auth_info: Mutex::new(auth_info),
            credentials,
            config,
            #[cfg(feature = "disk-cache")]
            disk_cache: None,
        }
    }

    /// Crée un nouveau client et authentifie avec les credentials fournis
    ///
    /// # Arguments
    ///
    /// * `username` - Email ou nom d'utilisateur Qobuz
    /// * `password` - Mot de passe
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmoqobuz::QobuzClient;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let client = QobuzClient::new("user@example.com", "password").await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn new(username: &str, password: &str) -> Result<Self> {
        Self::with_app_id(DEFAULT_APP_ID, username, password).await
    }

    /// Crée un nouveau client avec un App ID personnalisé
    pub async fn with_app_id(app_id: &str, username: &str, password: &str) -> Result<Self> {
        info!("Creating Qobuz client with app ID: {}", app_id);

        let api = QobuzApi::new(app_id)?;
        let auth_info = api.login(username, password).await?;

        let client = Self::build_client(
            api,
            Some(auth_info),
            Some((username.to_string(), password.to_string())),
            None,
        );

        Ok(client.finalize_disk_cache().await)
    }

    /// Crée un client en utilisant la configuration de pmoconfig
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmoqobuz::QobuzClient;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let client = QobuzClient::from_config().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn from_config() -> Result<Self> {
        let config = pmoconfig::get_config();
        Self::from_config_obj(config.as_ref()).await
    }

    /// Crée un client depuis un objet Config spécifique
    ///
    /// Cette méthode récupère les credentials, l'App ID et optionnellement
    /// le secret depuis la configuration.
    ///
    /// Ordre de priorité pour l'initialisation :
    /// 0. **Vérifier le cache du token d'authentification** (évite un login si token valide)
    /// 1. Si `appid` ET `secret` configurés → teste d'abord avec ces credentials
    /// 2. Si échec d'authentification → utilise le Spoofer pour obtenir de nouveaux credentials
    /// 3. Si aucun `appid`/`secret` configuré → utilise directement le Spoofer
    /// 4. Fallback ultime → utilise DEFAULT_APP_ID sans secret (requêtes signées échoueront)
    pub async fn from_config_obj(config: &Config) -> Result<Self> {
        let (username, password) = config.get_qobuz_credentials()?;
        let credentials = (username.clone(), password.clone());
        let config_arc = Arc::new(config.clone());

        let config_appid = config.get_qobuz_appid()?;
        let config_secret = config.get_qobuz_secret()?;
        let config_spoofer_secret = config.get_qobuz_spoofer_secret()?;

        let mut used_config_credentials = false;

        let mut api = match (config_appid.clone(), config_spoofer_secret, config_secret) {
            // Priority 1: Try memorized Spoofer secret (raw, no XOR)
            (Some(app_id), Some(spoofer_secret), _) => {
                info!("Trying memorized Spoofer secret with App ID: {}", app_id);
                match QobuzApi::with_raw_secret(&app_id, &spoofer_secret) {
                    Ok(api) => {
                        used_config_credentials = true;
                        api
                    }
                    Err(e) => {
                        info!(
                            "✗ Memorized Spoofer secret failed: {}. Re-fetching from Spoofer...",
                            e
                        );
                        Self::try_spoofer_fallback(config).await?
                    }
                }
            }
            // Priority 2: Try XOR secret (legacy configvalue)
            (Some(app_id), None, Some(secret)) => {
                info!(
                    "Creating Qobuz API with configured App ID: {} and XOR secret",
                    app_id
                );
                match QobuzApi::with_secret(&app_id, &secret) {
                    Ok(api) => {
                        used_config_credentials = true;
                        api
                    }
                    Err(e) => {
                        info!(
                            "✗ Failed to create API with configured credentials: {}. Falling back to Spoofer...",
                            e
                        );
                        Self::try_spoofer_fallback(config).await?
                    }
                }
            }
            // Priority 3: Fallback to Spoofer
            _ => {
                info!("AppID or secret not configured, using Spoofer...");
                Self::try_spoofer_fallback(config).await?
            }
        };

        if config.is_qobuz_auth_valid() {
            match (config.get_qobuz_auth_token(), config.get_qobuz_user_id()) {
                (Ok(Some(token)), Ok(Some(user_id)))
                    if !token.is_empty() && !user_id.is_empty() =>
                {
                    info!("✓ Reusing authentication token (optimistic, no login)");
                    api.set_auth_token(token.clone(), user_id.clone());

                    let auth_info = AuthInfo {
                        token,
                        user_id,
                        subscription_label: config.get_qobuz_subscription_label().ok().flatten(),
                    };

                    let client = Self::build_client(
                        api,
                        Some(auth_info),
                        Some(credentials.clone()),
                        Some(config_arc.clone()),
                    );

                    return Ok(client.finalize_disk_cache().await);
                }
                _ => {
                    debug!(
                        "Auth marked valid in config but token/user_id missing or invalid, performing login"
                    );
                }
            }
        }

        // Authentifier l'utilisateur
        let mut auth_result = api.login(&username, &password).await;

        if used_config_credentials {
            if let Err(err) = &auth_result {
                if err.is_auth_error() {
                    info!("✗ Configured credentials failed authentication: {}", err);
                    info!("→ Falling back to Spoofer to obtain new credentials...");
                    api = Self::try_spoofer_fallback(config).await?;
                    auth_result = api.login(&username, &password).await;
                }
            }
        }

        let auth_info = auth_result?;

        Self::persist_auth_info(config, &auth_info);

        let client = Self::build_client(api, Some(auth_info), Some(credentials), Some(config_arc));

        Ok(client.finalize_disk_cache().await)
    }

    /// Tente d'utiliser le Spoofer pour obtenir des credentials valides
    ///
    /// Cette méthode est appelée soit :
    /// - Quand aucun appid/secret n'est configuré
    /// - Quand les credentials configurés sont invalides/expirés
    async fn try_spoofer_fallback(config: &Config) -> Result<QobuzApi> {
        if let Some((app_id, secret)) = Self::fetch_spoofer_credentials(config).await? {
            // Use raw secret from Spoofer (no XOR)
            return QobuzApi::with_raw_secret(app_id, &secret);
        }

        info!(
            "✗ No valid secret found from Spoofer, falling back to DEFAULT_APP_ID without secret"
        );
        QobuzApi::new(DEFAULT_APP_ID)
    }

    async fn fetch_spoofer_credentials(config: &Config) -> Result<Option<(String, String)>> {
        match crate::api::Spoofer::new().await {
            Ok(spoofer) => match spoofer.get_app_id() {
                Ok(app_id) => {
                    // Use timezone secrets (like Python)
                    // Note: App Secret from bundle doesn't work for signed requests
                    match spoofer.get_secrets() {
                        Ok(secrets) => {
                            info!("Testing {} timezone secret(s)...", secrets.len());
                            let (username, password) = config.get_qobuz_credentials()?;

                            // Optimization: Login once with first secret to get auth token
                            // Then test all secrets using the same token
                            if let Some((first_timezone, first_secret)) = secrets.first() {
                                if let Ok(temp_api) =
                                    QobuzApi::with_raw_secret(&app_id, first_secret)
                                {
                                    if let Ok(_auth_info) =
                                        temp_api.login(&username, &password).await
                                    {
                                        // Now test each secret with the authenticated token
                                        for (timezone, secret) in secrets.iter() {
                                            debug!("Testing timezone secret: {}", timezone);

                                            if let Ok(test_api) =
                                                QobuzApi::with_raw_secret(&app_id, secret)
                                            {
                                                // Set the auth token from our initial login
                                                test_api.set_auth_token(
                                                    temp_api.auth_token().unwrap(),
                                                    temp_api.user_id().unwrap(),
                                                );

                                                // Test the secret using track/getFileUrl (like qobuz-player-client)
                                                // Use the same hardcoded track_id (64868955) as qobuz-player-client
                                                if test_api.get_file_url("64868955").await.is_ok() {
                                                    info!(
                                                        "✓ Secret from timezone '{}' works!",
                                                        timezone
                                                    );

                                                    // Save both appid and the working secret
                                                    if let Err(e) = config.set_qobuz_appid(&app_id)
                                                    {
                                                        debug!("Could not save appid: {}", e);
                                                    }
                                                    if let Err(e) =
                                                        config.set_qobuz_spoofer_secret(secret)
                                                    {
                                                        debug!(
                                                            "Could not save spoofer secret: {}",
                                                            e
                                                        );
                                                    }

                                                    return Ok(Some((
                                                        app_id.clone(),
                                                        secret.clone(),
                                                    )));
                                                } else {
                                                    debug!("✗ Secret from timezone '{}' failed track/getFileUrl test", timezone);
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            info!("✗ No valid secret found");
                            Ok(None)
                        }
                        Err(e) => {
                            info!("Failed to extract timezone secrets: {}", e);
                            Ok(None)
                        }
                    }
                }
                Err(e) => {
                    info!(
                        "Spoofer failed to extract app_id: {}, falling back to DEFAULT_APP_ID",
                        e
                    );
                    Ok(None)
                }
            },
            Err(e) => {
                info!(
                    "Spoofer failed: {}, falling back to DEFAULT_APP_ID without secret",
                    e
                );
                Ok(None)
            }
        }
    }

    async fn refresh_credentials_via_spoofer(&self) -> Result<()> {
        let config_arc = match &self.config {
            Some(cfg) => cfg.clone(),
            None => pmoconfig::get_config(),
        };

        match Self::fetch_spoofer_credentials(config_arc.as_ref()).await? {
            Some((app_id, secret)) => {
                // Use raw secret (no XOR) from Spoofer
                self.api.update_credentials_raw(app_id, &secret);
                info!("✓ Updated Qobuz API credentials using Spoofer");
                Ok(())
            }
            None => Err(QobuzError::Configuration(
                "Unable to refresh Qobuz credentials via Spoofer".to_string(),
            )),
        }
    }

    /// Définit le format audio par défaut
    pub fn set_format(&mut self, format: AudioFormat) {
        self.api.set_format(format);
    }

    /// Retourne le format audio configuré
    pub fn format(&self) -> AudioFormat {
        self.api.format()
    }

    /// Retourne les informations d'authentification
    pub fn auth_info(&self) -> Option<AuthInfo> {
        self.auth_info.lock().unwrap().clone()
    }

    #[cfg(feature = "disk-cache")]
    fn user_id(&self) -> Option<String> {
        self.auth_info
            .lock()
            .unwrap()
            .as_ref()
            .map(|info| info.user_id.clone())
    }

    /// Retourne une référence au cache
    pub fn cache(&self) -> Arc<QobuzCache> {
        self.cache.clone()
    }

    #[cfg(feature = "disk-cache")]
    pub async fn purge_disk_cache(&self) -> Result<usize> {
        if let Some(disk) = &self.disk_cache {
            disk.purge_expired()
                .await
                .map_err(|err| QobuzError::Cache(err.to_string()))
        } else {
            Ok(0)
        }
    }

    #[cfg(feature = "disk-cache")]
    pub fn with_disk_cache(mut self, store: Arc<dyn crate::disk_cache::CacheStore>) -> Self {
        self.disk_cache = Some(store);
        self
    }

    #[cfg(feature = "disk-cache")]
    fn attach_default_disk_cache(mut self) -> Self {
        if let Some(store) = Self::default_disk_cache_store(self.config.as_deref()) {
            self = self.with_disk_cache(store);
        }
        self
    }

    #[cfg(not(feature = "disk-cache"))]
    fn attach_default_disk_cache(self) -> Self {
        self
    }

    #[cfg(feature = "disk-cache")]
    async fn finalize_disk_cache(self) -> Self {
        let client = self.attach_default_disk_cache();
        if let Err(err) = client.purge_disk_cache().await {
            debug!("Failed to purge disk cache on startup: {}", err);
        }
        client
    }

    #[cfg(not(feature = "disk-cache"))]
    async fn finalize_disk_cache(self) -> Self {
        self.attach_default_disk_cache()
    }

    #[cfg(feature = "disk-cache")]
    fn default_disk_cache_store(
        config: Option<&Config>,
    ) -> Option<Arc<dyn crate::disk_cache::CacheStore>> {
        let cache_dir = match config {
            Some(cfg) => match cfg.get_qobuz_cache_dir() {
                Ok(dir) => dir,
                Err(err) => {
                    debug!("Failed to read qobuz cache dir from config: {}", err);
                    return None;
                }
            },
            None => {
                let global = pmoconfig::get_config();
                match global.get_qobuz_cache_dir() {
                    Ok(dir) => dir,
                    Err(err) => {
                        debug!("Failed to read qobuz cache dir from global config: {}", err);
                        return None;
                    }
                }
            }
        };

        let dir_path = std::path::PathBuf::from(&cache_dir);
        if let Err(err) = std::fs::create_dir_all(&dir_path) {
            debug!(
                "Failed to create disk cache directory {}: {}",
                dir_path.display(),
                err
            );
            return None;
        }

        let db_path = dir_path.join("qobuz_cache.sqlite");

        match crate::disk_cache::SqliteCacheStore::new(db_path) {
            Ok(store) => {
                let store: Arc<dyn crate::disk_cache::CacheStore> = Arc::new(store);
                Some(store)
            }
            Err(err) => {
                debug!("Failed to initialize SQLite disk cache: {}", err);
                None
            }
        }
    }

    async fn call_with_auth_repair<T, F, Fut>(&self, operation: &str, mut op: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T>> + Send,
    {
        match op().await {
            Ok(result) => Ok(result),
            Err(err) if err.is_signature_error() => {
                warn!(
                    "Request signature error during {}. Qobuz app secret may be outdated.",
                    operation
                );
                if let Err(refresh_err) = self.refresh_credentials_via_spoofer().await {
                    warn!(
                        "Failed to refresh Qobuz credentials automatically: {}",
                        refresh_err
                    );
                    Err(err)
                } else {
                    info!(
                        "Successfully refreshed Qobuz credentials. Retrying {}...",
                        operation
                    );
                    op().await
                }
            }
            Err(err) if err.is_auth_error() => {
                info!(
                    "Authentication error during {}. Attempting automatic repair...",
                    operation
                );
                self.repair_auth().await?;
                op().await
            }
            Err(err) => Err(err),
        }
    }

    async fn repair_auth(&self) -> Result<()> {
        let (username, password) = self.credentials.as_ref().cloned().ok_or_else(|| {
            QobuzError::Unauthorized(
                "Cannot repair authentication without stored credentials".to_string(),
            )
        })?;

        let auth_info = self.api.login(&username, &password).await?;

        if let Some(config) = &self.config {
            Self::persist_auth_info(config.as_ref(), &auth_info);
        }

        let mut guard = self.auth_info.lock().unwrap();
        *guard = Some(auth_info);
        Ok(())
    }

    fn persist_auth_info(config: &Config, auth_info: &AuthInfo) {
        use std::time::{Duration, SystemTime, UNIX_EPOCH};

        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + Duration::from_secs(24 * 3600).as_secs(); // 24h

        if let Err(e) = config.set_qobuz_auth_info(
            &auth_info.token,
            &auth_info.user_id,
            auth_info.subscription_label.as_deref(),
            expires_at,
        ) {
            debug!("Failed to save authentication to config: {}", e);
        } else {
            info!("✓ Saved authentication token to configuration");
        }
    }

    // ============ Albums ============

    /// Récupère un album par son ID
    pub async fn get_album(&self, album_id: &str) -> Result<Album> {
        // Vérifier le cache d'abord
        if let Some(album) = self.cache.get_album(album_id).await {
            debug!("Album {} found in cache", album_id);
            return Ok(album);
        }

        // Sinon, récupérer depuis l'API
        let album = self
            .call_with_auth_repair("get_album", || self.api.get_album(album_id))
            .await?;

        // Mettre en cache
        self.cache
            .put_album(album_id.to_string(), album.clone())
            .await;

        Ok(album)
    }

    /// Récupère les tracks d'un album
    pub async fn get_album_tracks(&self, album_id: &str) -> Result<Vec<Track>> {
        // Vérifier le cache d'abord
        if let Some(tracks) = self.cache.get_album_tracks(album_id).await {
            debug!("Album tracks for {} found in cache", album_id);
            return Ok(tracks);
        }

        // Sinon, récupérer depuis l'API
        let tracks = self
            .call_with_auth_repair("get_album_tracks", || self.api.get_album_tracks(album_id))
            .await?;

        // Mettre les tracks en cache (individuellement ET la liste complète)
        for track in &tracks {
            self.cache.put_track(track.id.clone(), track.clone()).await;
        }
        self.cache
            .put_album_tracks(album_id.to_string(), tracks.clone())
            .await;

        Ok(tracks)
    }

    // ============ Tracks ============

    /// Récupère une track par son ID
    pub async fn get_track(&self, track_id: &str) -> Result<Track> {
        if let Some(track) = self.cache.get_track(track_id).await {
            debug!("Track {} found in cache", track_id);
            return Ok(track);
        }

        let track = self
            .call_with_auth_repair("get_track", || self.api.get_track(track_id))
            .await?;
        self.cache
            .put_track(track_id.to_string(), track.clone())
            .await;

        Ok(track)
    }

    /// Récupère l'URL de streaming d'une track
    pub async fn get_stream_url(&self, track_id: &str) -> Result<String> {
        // Vérifier le cache d'abord
        if let Some(info) = self.cache.get_stream_url(track_id).await {
            if info.expires_at > chrono::Utc::now() {
                debug!("Stream URL for track {} found in cache", track_id);
                return Ok(info.url);
            }
        }

        // Sinon, récupérer depuis l'API
        let info = self
            .call_with_auth_repair("get_file_url", || self.api.get_file_url(track_id))
            .await?;
        let url = info.url.clone();

        // Mettre en cache
        self.cache.put_stream_url(track_id.to_string(), info).await;

        Ok(url)
    }

    // ============ Artists ============

    /// Récupère un artiste par son ID
    pub async fn get_artist(&self, artist_id: &str) -> Result<Artist> {
        if let Some(artist) = self.cache.get_artist(artist_id).await {
            debug!("Artist {} found in cache", artist_id);
            return Ok(artist);
        }

        // Pour récupérer un artiste, on doit passer par get_artist_albums
        let albums = self
            .call_with_auth_repair("get_artist_albums_for_artist", || {
                self.api.get_artist_albums(artist_id)
            })
            .await?;

        if let Some(first_album) = albums.first() {
            let artist = first_album.artist.clone();
            self.cache
                .put_artist(artist_id.to_string(), artist.clone())
                .await;
            Ok(artist)
        } else {
            Err(QobuzError::NotFound(format!(
                "Artist {} not found",
                artist_id
            )))
        }
    }

    /// Récupère les albums d'un artiste
    pub async fn get_artist_albums(&self, artist_id: &str) -> Result<Vec<Album>> {
        self.call_with_auth_repair("get_artist_albums", || {
            self.api.get_artist_albums(artist_id)
        })
        .await
    }

    /// Récupère les artistes similaires
    pub async fn get_similar_artists(&self, artist_id: &str) -> Result<Vec<Artist>> {
        self.call_with_auth_repair("get_similar_artists", || {
            self.api.get_similar_artists(artist_id)
        })
        .await
    }

    // ============ Playlists ============

    /// Récupère une playlist par son ID
    pub async fn get_playlist(&self, playlist_id: &str) -> Result<Playlist> {
        if let Some(playlist) = self.cache.get_playlist(playlist_id).await {
            debug!("Playlist {} found in cache", playlist_id);
            return Ok(playlist);
        }

        let playlist = self
            .call_with_auth_repair("get_playlist", || self.api.get_playlist(playlist_id))
            .await?;
        self.cache
            .put_playlist(playlist_id.to_string(), playlist.clone())
            .await;

        Ok(playlist)
    }

    /// Récupère les tracks d'une playlist
    pub async fn get_playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        self.call_with_auth_repair("get_playlist_tracks", || {
            self.api.get_playlist_tracks(playlist_id)
        })
        .await
    }

    // ============ Catalogue ============

    /// Récupère la liste des genres
    pub async fn get_genres(&self) -> Result<Vec<Genre>> {
        self.call_with_auth_repair("get_genres", || self.api.get_genres())
            .await
    }

    /// Récupère les albums featured (nouveautés, éditeur, etc.)
    pub async fn get_featured_albums(
        &self,
        genre_id: Option<&str>,
        type_: &str,
    ) -> Result<Vec<Album>> {
        self.call_with_auth_repair("get_featured_albums", || {
            self.api.get_featured_albums(genre_id, type_)
        })
        .await
    }

    /// Récupère les playlists featured
    pub async fn get_featured_playlists(
        &self,
        genre_id: Option<&str>,
        tags: Option<&str>,
    ) -> Result<Vec<Playlist>> {
        self.call_with_auth_repair("get_featured_playlists", || {
            self.api.get_featured_playlists(genre_id, tags)
        })
        .await
    }

    /// Récupère les artistes featured
    pub async fn get_featured_artists(
        &self,
        genre_id: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<Artist>> {
        self.call_with_auth_repair("get_featured_artists", || {
            self.api.get_featured_artists(genre_id, limit, offset)
        })
        .await
    }

    // ============ Recherche ============

    /// Recherche dans le catalogue Qobuz
    ///
    /// # Arguments
    ///
    /// * `query` - Termes de recherche
    /// * `type_` - Type de recherche : None (tous), Some("albums"), Some("artists"), Some("tracks"), Some("playlists")
    pub async fn search(&self, query: &str, type_: Option<&str>) -> Result<SearchResult> {
        // Créer une clé de cache
        let cache_key = format!("{}:{}", query, type_.unwrap_or("all"));

        // Vérifier le cache
        if let Some(result) = self.cache.get_search(&cache_key).await {
            debug!("Search results for '{}' found in cache", query);
            return Ok(result);
        }

        // Sinon, rechercher via l'API
        let result = self
            .call_with_auth_repair("search", || self.api.search(query, type_))
            .await?;

        // Mettre en cache
        self.cache.put_search(cache_key, result.clone()).await;

        Ok(result)
    }

    /// Recherche des albums
    pub async fn search_albums(&self, query: &str) -> Result<Vec<Album>> {
        let result = self.search(query, Some("albums")).await?;
        Ok(result.albums)
    }

    /// Recherche des artistes
    pub async fn search_artists(&self, query: &str) -> Result<Vec<Artist>> {
        let result = self.search(query, Some("artists")).await?;
        Ok(result.artists)
    }

    /// Recherche des tracks
    pub async fn search_tracks(&self, query: &str) -> Result<Vec<Track>> {
        let result = self.search(query, Some("tracks")).await?;
        Ok(result.tracks)
    }

    /// Recherche des playlists
    pub async fn search_playlists(&self, query: &str) -> Result<Vec<Playlist>> {
        let result = self.search(query, Some("playlists")).await?;
        Ok(result.playlists)
    }

    // ============ Favoris ============

    /// Récupère les albums favoris de l'utilisateur
    pub async fn get_favorite_albums(&self) -> Result<Vec<Album>> {
        #[cfg(feature = "disk-cache")]
        let user_id = self
            .user_id()
            .ok_or_else(|| QobuzError::Unauthorized("Missing authenticated user ID".into()))?;

        #[cfg(feature = "disk-cache")]
        let ttl = std::time::Duration::from_secs(6 * 3600);

        #[cfg(feature = "disk-cache")]
        if let Some(disk) = &self.disk_cache {
            if let Some(entry) = disk
                .get_json::<Vec<Album>>(&user_id, "favorites_albums", "all")
                .await?
            {
                if entry.fresh {
                    return Ok(entry.value);
                }
            }
        }

        let albums = self
            .call_with_auth_repair("get_favorite_albums", || self.api.get_favorite_albums())
            .await?;

        #[cfg(feature = "disk-cache")]
        if let Some(disk) = &self.disk_cache {
            let _ = disk
                .put_json(&user_id, "favorites_albums", "all", ttl, &albums)
                .await;
        }

        Ok(albums)
    }

    /// Récupère les artistes favoris de l'utilisateur
    pub async fn get_favorite_artists(&self) -> Result<Vec<Artist>> {
        self.call_with_auth_repair("get_favorite_artists", || self.api.get_favorite_artists())
            .await
    }

    /// Récupère les tracks favorites de l'utilisateur
    pub async fn get_favorite_tracks(&self) -> Result<Vec<Track>> {
        #[cfg(feature = "disk-cache")]
        let user_id = self
            .user_id()
            .ok_or_else(|| QobuzError::Unauthorized("Missing authenticated user ID".into()))?;

        #[cfg(feature = "disk-cache")]
        let ttl = std::time::Duration::from_secs(6 * 3600);

        #[cfg(feature = "disk-cache")]
        if let Some(disk) = &self.disk_cache {
            if let Some(entry) = disk
                .get_json::<Vec<Track>>(&user_id, "favorites_tracks", "all")
                .await?
            {
                if entry.fresh {
                    return Ok(entry.value);
                }
            }
        }

        let tracks = self
            .call_with_auth_repair("get_favorite_tracks", || self.api.get_favorite_tracks())
            .await?;

        #[cfg(feature = "disk-cache")]
        if let Some(disk) = &self.disk_cache {
            let _ = disk
                .put_json(&user_id, "favorites_tracks", "all", ttl, &tracks)
                .await;
        }

        Ok(tracks)
    }

    /// Récupère les playlists de l'utilisateur
    pub async fn get_user_playlists(&self) -> Result<Vec<Playlist>> {
        #[cfg(feature = "disk-cache")]
        let user_id = self
            .user_id()
            .ok_or_else(|| QobuzError::Unauthorized("Missing authenticated user ID".into()))?;

        #[cfg(feature = "disk-cache")]
        let ttl = std::time::Duration::from_secs(6 * 3600);

        #[cfg(feature = "disk-cache")]
        if let Some(disk) = &self.disk_cache {
            if let Some(entry) = disk
                .get_json::<Vec<Playlist>>(&user_id, "user_playlists", "all")
                .await?
            {
                if entry.fresh {
                    return Ok(entry.value);
                }
            }
        }

        let playlists = self
            .call_with_auth_repair("get_user_playlists", || self.api.get_user_playlists())
            .await?;

        #[cfg(feature = "disk-cache")]
        if let Some(disk) = &self.disk_cache {
            let _ = disk
                .put_json(&user_id, "user_playlists", "all", ttl, &playlists)
                .await;
        }

        Ok(playlists)
    }

    /// Ajoute un album aux favoris
    pub async fn add_favorite_album(&self, album_id: &str) -> Result<()> {
        self.call_with_auth_repair("add_favorite_album", || {
            self.api.add_favorite_album(album_id)
        })
        .await
    }

    /// Supprime un album des favoris
    pub async fn remove_favorite_album(&self, album_id: &str) -> Result<()> {
        self.call_with_auth_repair("remove_favorite_album", || {
            self.api.remove_favorite_album(album_id)
        })
        .await
    }

    /// Ajoute un track aux favoris
    pub async fn add_favorite_track(&self, track_id: &str) -> Result<()> {
        self.call_with_auth_repair("add_favorite_track", || {
            self.api.add_favorite_track(track_id)
        })
        .await
    }

    /// Supprime un track des favoris
    pub async fn remove_favorite_track(&self, track_id: &str) -> Result<()> {
        self.call_with_auth_repair("remove_favorite_track", || {
            self.api.remove_favorite_track(track_id)
        })
        .await
    }

    /// Ajoute un track à une playlist
    pub async fn add_to_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()> {
        self.call_with_auth_repair("add_to_playlist", || {
            self.api.add_to_playlist(playlist_id, track_id)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_ext::QobuzConfigExt;

    #[test]
    fn test_audio_format() {
        assert_eq!(AudioFormat::default(), AudioFormat::Flac_Lossless);
    }

    #[tokio::test]
    async fn from_config_reuses_token_without_login() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().to_string_lossy().to_string();
        let config = pmoconfig::Config::load_config(&config_path).unwrap();

        config.set_qobuz_username("user@example.com").unwrap();
        config.set_qobuz_password("password").unwrap();
        config.set_qobuz_appid("1401488693436528").unwrap();
        // base64 for "secret"
        config.set_qobuz_secret("c2VjcmV0").unwrap();
        config
            .set_qobuz_auth_info("token123", "user123", Some("Hi-Fi"), 1_700_000_000)
            .unwrap();

        let client = QobuzClient::from_config_obj(&config).await.unwrap();
        let auth_info = client.auth_info().expect("auth info");

        assert_eq!(auth_info.token, "token123");
        assert_eq!(auth_info.user_id, "user123");
    }
}
