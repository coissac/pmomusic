//! Client principal pour interagir avec l'API Qobuz
//!
//! Ce module fournit un client haut-niveau avec authentification et cache intégré.

use crate::api::auth::AuthInfo;
use crate::api::{QobuzApi, DEFAULT_APP_ID};
use crate::cache::QobuzCache;
use crate::config_ext::QobuzConfigExt;
use crate::error::{QobuzError, Result};
use crate::models::*;
use pmoconfig::Config;
use std::sync::Arc;
use tracing::{debug, info};

/// Client Qobuz haut-niveau avec cache
pub struct QobuzClient {
    /// API bas-niveau
    api: QobuzApi,
    /// Cache en mémoire
    cache: Arc<QobuzCache>,
    /// Informations d'authentification
    auth_info: Option<AuthInfo>,
}

impl QobuzClient {
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

        let mut api = QobuzApi::new(app_id)?;
        let auth_info = api.login(username, password).await?;

        Ok(Self {
            api,
            cache: Arc::new(QobuzCache::new()),
            auth_info: Some(auth_info),
        })
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

        // Étape 0 : Essayer de réutiliser le token stocké dans la configuration
        // Note: On ne vérifie PAS l'expiration - si le token est invalide, les requêtes
        // échoueront avec 401/403 et déclencheront un re-login automatique
        if let (Ok(Some(token)), Ok(Some(user_id))) =
            (config.get_qobuz_auth_token(), config.get_qobuz_user_id())
        {
            info!("✓ Found stored authentication token in configuration");

            // Récupérer l'App ID et le secret depuis la config pour créer l'API
            let config_appid = config.get_qobuz_appid()?;
            let config_secret = config.get_qobuz_secret()?;

            match (config_appid, config_secret) {
                (Some(app_id), Some(secret)) => match QobuzApi::with_secret(&app_id, &secret) {
                    Ok(mut api) => {
                        // Réutiliser le token de la configuration
                        api.set_auth_token(token.clone(), user_id.clone());

                        info!("✓ Reusing authentication token (no login required)");
                        info!("   → Token will be validated on first API request");

                        let auth_info = AuthInfo {
                            token,
                            user_id,
                            subscription_label: config.get_qobuz_subscription_label().ok().flatten(),
                        };

                        return Ok(Self {
                            api,
                            cache: Arc::new(QobuzCache::new()),
                            auth_info: Some(auth_info),
                        });
                    }
                    Err(e) => {
                        debug!("Failed to create API with stored credentials: {}", e);
                        info!("→ Credentials in config are invalid, will perform login");
                        // Continuer vers le login normal
                    }
                },
                _ => {
                    debug!("No appid/secret in config, cannot reuse token");
                    info!("→ Missing AppID/secret, will perform login");
                    // Continuer vers le login normal
                }
            }
        } else {
            debug!("No stored authentication token found in configuration, will perform login");
        }

        // Récupérer l'App ID et le secret depuis la config
        let config_appid = config.get_qobuz_appid()?;
        let config_secret = config.get_qobuz_secret()?;

        // Déterminer comment créer l'API
        let mut api = match (config_appid, config_secret) {
            // Cas 1: AppID ET secret configurés → test avec authentification
            (Some(app_id), Some(secret)) => {
                info!(
                    "Creating Qobuz API with configured App ID: {} and secret",
                    app_id
                );

                match QobuzApi::with_secret(&app_id, &secret) {
                    Ok(mut test_api) => {
                        // Tenter l'authentification pour valider les credentials
                        debug!("Testing configured credentials with login...");
                        match test_api.login(&username, &password).await {
                            Ok(auth_info) => {
                                info!("✓ Configured credentials are valid");

                                // Sauvegarder le token dans la configuration
                                use std::time::{SystemTime, UNIX_EPOCH, Duration};
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

                                // Les credentials sont valides, retourner directement
                                return Ok(Self {
                                    api: test_api,
                                    cache: Arc::new(QobuzCache::new()),
                                    auth_info: Some(auth_info),
                                });
                            }
                            Err(e) if e.is_auth_error() => {
                                info!("✗ Configured credentials failed authentication: {}", e);
                                info!("→ Falling back to Spoofer to obtain new credentials...");
                                // Continuer vers le Spoofer (voir après le match)
                            }
                            Err(e) => {
                                // Autre erreur (réseau, etc.) → propager
                                return Err(e);
                            }
                        }
                    }
                    Err(e) => {
                        info!("✗ Failed to create API with configured credentials: {}", e);
                        info!("→ Falling back to Spoofer...");
                        // Continuer vers le Spoofer
                    }
                }

                // Si on arrive ici, les credentials configurés ont échoué
                // → Appel du Spoofer
                Self::try_spoofer_fallback(config).await?
            }

            // Cas 2: Aucun ou seulement l'un des deux → utiliser directement le Spoofer
            _ => {
                info!("AppID or secret not configured, using Spoofer to obtain valid credentials...");
                Self::try_spoofer_fallback(config).await?
            }
        };

        // Authentifier l'utilisateur
        let auth_info = api.login(&username, &password).await?;

        // Sauvegarder le token dans la configuration pour éviter de re-login la prochaine fois
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

        Ok(Self {
            api,
            cache: Arc::new(QobuzCache::new()),
            auth_info: Some(auth_info),
        })
    }

    /// Tente d'utiliser le Spoofer pour obtenir des credentials valides
    ///
    /// Cette méthode est appelée soit :
    /// - Quand aucun appid/secret n'est configuré
    /// - Quand les credentials configurés sont invalides/expirés
    async fn try_spoofer_fallback(config: &Config) -> Result<QobuzApi> {
        match crate::api::Spoofer::new().await {
            Ok(spoofer) => {
                match spoofer.get_app_id() {
                    Ok(app_id) => {
                        info!("Spoofer found App ID: {}", app_id);

                        match spoofer.get_secrets() {
                            Ok(secrets) => {
                                info!("Spoofer found {} secret(s), testing them...", secrets.len());

                                // Tester chaque secret pour trouver celui qui fonctionne
                                for (timezone, secret) in secrets.iter() {
                                    debug!("Testing secret for timezone: {}", timezone);

                                    match QobuzApi::with_secret(&app_id, secret) {
                                        Ok(test_api) => {
                                            info!("✓ Successfully created API with secret from timezone: {}", timezone);

                                            // Sauvegarder les credentials valides dans la config
                                            if let Err(e) = config.set_qobuz_appid(&app_id) {
                                                debug!("Could not save appid to config: {}", e);
                                            }
                                            if let Err(e) = config.set_qobuz_secret(secret) {
                                                debug!("Could not save secret to config: {}", e);
                                            }

                                            return Ok(test_api);
                                        }
                                        Err(e) => {
                                            debug!("Failed to create API with secret from {}: {}", timezone, e);
                                            continue;
                                        }
                                    }
                                }

                                // Si aucun secret n'a fonctionné, utiliser le fallback
                                info!("✗ No valid secret found from Spoofer, falling back to DEFAULT_APP_ID without secret");
                                QobuzApi::new(DEFAULT_APP_ID)
                            }
                            Err(e) => {
                                info!("Spoofer failed to extract secrets: {}, falling back to DEFAULT_APP_ID", e);
                                QobuzApi::new(DEFAULT_APP_ID)
                            }
                        }
                    }
                    Err(e) => {
                        info!("Spoofer failed to extract app_id: {}, falling back to DEFAULT_APP_ID", e);
                        QobuzApi::new(DEFAULT_APP_ID)
                    }
                }
            }
            Err(e) => {
                info!("Spoofer failed: {}, falling back to DEFAULT_APP_ID without secret", e);
                QobuzApi::new(DEFAULT_APP_ID)
            }
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
    pub fn auth_info(&self) -> Option<&AuthInfo> {
        self.auth_info.as_ref()
    }

    /// Retourne une référence au cache
    pub fn cache(&self) -> Arc<QobuzCache> {
        self.cache.clone()
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
        let album = self.api.get_album(album_id).await?;

        // Mettre en cache
        self.cache
            .put_album(album_id.to_string(), album.clone())
            .await;

        Ok(album)
    }

    /// Récupère les tracks d'un album
    pub async fn get_album_tracks(&self, album_id: &str) -> Result<Vec<Track>> {
        let tracks = self.api.get_album_tracks(album_id).await?;

        // Mettre les tracks en cache
        for track in &tracks {
            self.cache.put_track(track.id.clone(), track.clone()).await;
        }

        Ok(tracks)
    }

    // ============ Tracks ============

    /// Récupère une track par son ID
    pub async fn get_track(&self, track_id: &str) -> Result<Track> {
        if let Some(track) = self.cache.get_track(track_id).await {
            debug!("Track {} found in cache", track_id);
            return Ok(track);
        }

        let track = self.api.get_track(track_id).await?;
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
        let info = self.api.get_file_url(track_id).await?;
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
        let albums = self.api.get_artist_albums(artist_id).await?;

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
        self.api.get_artist_albums(artist_id).await
    }

    /// Récupère les artistes similaires
    pub async fn get_similar_artists(&self, artist_id: &str) -> Result<Vec<Artist>> {
        self.api.get_similar_artists(artist_id).await
    }

    // ============ Playlists ============

    /// Récupère une playlist par son ID
    pub async fn get_playlist(&self, playlist_id: &str) -> Result<Playlist> {
        if let Some(playlist) = self.cache.get_playlist(playlist_id).await {
            debug!("Playlist {} found in cache", playlist_id);
            return Ok(playlist);
        }

        let playlist = self.api.get_playlist(playlist_id).await?;
        self.cache
            .put_playlist(playlist_id.to_string(), playlist.clone())
            .await;

        Ok(playlist)
    }

    /// Récupère les tracks d'une playlist
    pub async fn get_playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        self.api.get_playlist_tracks(playlist_id).await
    }

    // ============ Catalogue ============

    /// Récupère la liste des genres
    pub async fn get_genres(&self) -> Result<Vec<Genre>> {
        self.api.get_genres().await
    }

    /// Récupère les albums featured (nouveautés, éditeur, etc.)
    pub async fn get_featured_albums(
        &self,
        genre_id: Option<&str>,
        type_: &str,
    ) -> Result<Vec<Album>> {
        self.api.get_featured_albums(genre_id, type_).await
    }

    /// Récupère les playlists featured
    pub async fn get_featured_playlists(
        &self,
        genre_id: Option<&str>,
        tags: Option<&str>,
    ) -> Result<Vec<Playlist>> {
        self.api.get_featured_playlists(genre_id, tags).await
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
        let result = self.api.search(query, type_).await?;

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
        self.api.get_favorite_albums().await
    }

    /// Récupère les artistes favoris de l'utilisateur
    pub async fn get_favorite_artists(&self) -> Result<Vec<Artist>> {
        self.api.get_favorite_artists().await
    }

    /// Récupère les tracks favorites de l'utilisateur
    pub async fn get_favorite_tracks(&self) -> Result<Vec<Track>> {
        self.api.get_favorite_tracks().await
    }

    /// Récupère les playlists de l'utilisateur
    pub async fn get_user_playlists(&self) -> Result<Vec<Playlist>> {
        self.api.get_user_playlists().await
    }

    /// Ajoute un album aux favoris
    pub async fn add_favorite_album(&self, album_id: &str) -> Result<()> {
        self.api.add_favorite_album(album_id).await
    }

    /// Supprime un album des favoris
    pub async fn remove_favorite_album(&self, album_id: &str) -> Result<()> {
        self.api.remove_favorite_album(album_id).await
    }

    /// Ajoute un track aux favoris
    pub async fn add_favorite_track(&self, track_id: &str) -> Result<()> {
        self.api.add_favorite_track(track_id).await
    }

    /// Supprime un track des favoris
    pub async fn remove_favorite_track(&self, track_id: &str) -> Result<()> {
        self.api.remove_favorite_track(track_id).await
    }

    /// Ajoute un track à une playlist
    pub async fn add_to_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()> {
        self.api.add_to_playlist(playlist_id, track_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_format() {
        assert_eq!(AudioFormat::default(), AudioFormat::Flac_Lossless);
    }
}
