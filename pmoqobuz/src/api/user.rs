//! Module d'accès aux données utilisateur (favoris)

use super::catalog::{AlbumResponse, ArtistResponse, PlaylistResponse, TrackResponse};
use super::QobuzApi;
use crate::error::{QobuzError, Result};
use crate::models::*;
use serde::Deserialize;
use tracing::debug;

/// Réponse paginée
#[derive(Debug, Deserialize)]
struct PaginatedResponse<T> {
    items: Vec<T>,
}

/// Réponse de l'endpoint /favorite/getUserFavorites
#[derive(Debug, Deserialize)]
struct FavoritesResponse {
    #[serde(default)]
    albums: Option<PaginatedResponse<AlbumResponse>>,
    #[serde(default)]
    artists: Option<PaginatedResponse<ArtistResponse>>,
    #[serde(default)]
    tracks: Option<PaginatedResponse<TrackResponse>>,
}

/// Réponse de l'endpoint /playlist/getUserPlaylists
#[derive(Debug, Deserialize)]
struct UserPlaylistsResponse {
    playlists: PaginatedResponse<PlaylistResponse>,
}

impl QobuzApi {
    /// Vérifie que l'utilisateur est authentifié
    fn ensure_authenticated(&self) -> Result<String> {
        self.user_id()
            .ok_or_else(|| QobuzError::Unauthorized("Not authenticated".to_string()))
    }

    /// Récupère les albums favoris de l'utilisateur
    pub async fn get_favorite_albums(&self) -> Result<Vec<Album>> {
        let user_id = self.ensure_authenticated()?;
        debug!("Fetching favorite albums for user {}", user_id);

        let params = [
            ("user_id", user_id.as_str()),
            ("type", "albums"),
            ("limit", "1000"),
        ];

        let response: FavoritesResponse = self.get("/favorite/getUserFavorites", &params).await?;

        if let Some(albums) = response.albums {
            Ok(albums
                .items
                .into_iter()
                .map(QobuzApi::parse_album)
                .filter(|a| a.streamable)
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Récupère les artistes favoris de l'utilisateur
    pub async fn get_favorite_artists(&self) -> Result<Vec<Artist>> {
        let user_id = self.ensure_authenticated()?;
        debug!("Fetching favorite artists for user {}", user_id);

        let params = [
            ("user_id", user_id.as_str()),
            ("type", "artists"),
            ("limit", "1000"),
        ];

        let response: FavoritesResponse = self.get("/favorite/getUserFavorites", &params).await?;

        if let Some(artists) = response.artists {
            Ok(artists
                .items
                .into_iter()
                .map(QobuzApi::parse_artist)
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Récupère les tracks favorites de l'utilisateur
    pub async fn get_favorite_tracks(&self) -> Result<Vec<Track>> {
        let user_id = self.ensure_authenticated()?;
        debug!("Fetching favorite tracks for user {}", user_id);

        let params = [
            ("user_id", user_id.as_str()),
            ("type", "tracks"),
            ("limit", "1000"),
        ];

        let response: FavoritesResponse = self.get("/favorite/getUserFavorites", &params).await?;

        if let Some(tracks) = response.tracks {
            Ok(tracks
                .items
                .into_iter()
                .map(|t| QobuzApi::parse_track(t, None))
                .filter(|t| t.streamable)
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Récupère les playlists de l'utilisateur
    pub async fn get_user_playlists(&self) -> Result<Vec<Playlist>> {
        let user_id = self.ensure_authenticated()?;
        debug!("Fetching playlists for user {}", user_id);

        let params = [("user_id", user_id.as_str()), ("limit", "1000")];

        let response: UserPlaylistsResponse =
            self.get("/playlist/getUserPlaylists", &params).await?;

        Ok(response
            .playlists
            .items
            .into_iter()
            .map(QobuzApi::parse_playlist)
            .collect())
    }

    /// Ajoute un album aux favoris
    pub async fn add_favorite_album(&self, album_id: &str) -> Result<()> {
        let user_id = self.ensure_authenticated()?;
        debug!(
            "Adding album {} to favorites for user {}",
            album_id, user_id
        );

        let params = [("album_id", album_id), ("user_id", user_id.as_str())];

        self.get::<serde_json::Value>("/favorite/create", &params)
            .await?;
        Ok(())
    }

    /// Supprime un album des favoris
    pub async fn remove_favorite_album(&self, album_id: &str) -> Result<()> {
        let user_id = self.ensure_authenticated()?;
        debug!(
            "Removing album {} from favorites for user {}",
            album_id, user_id
        );

        let params = [("album_ids", album_id), ("user_id", user_id.as_str())];

        self.get::<serde_json::Value>("/favorite/delete", &params)
            .await?;
        Ok(())
    }

    /// Ajoute un track aux favoris
    pub async fn add_favorite_track(&self, track_id: &str) -> Result<()> {
        let user_id = self.ensure_authenticated()?;
        debug!(
            "Adding track {} to favorites for user {}",
            track_id, user_id
        );

        let params = [("track_id", track_id), ("user_id", user_id.as_str())];

        self.get::<serde_json::Value>("/favorite/create", &params)
            .await?;
        Ok(())
    }

    /// Supprime un track des favoris
    pub async fn remove_favorite_track(&self, track_id: &str) -> Result<()> {
        let user_id = self.ensure_authenticated()?;
        debug!(
            "Removing track {} from favorites for user {}",
            track_id, user_id
        );

        let params = [("track_ids", track_id), ("user_id", user_id.as_str())];

        self.get::<serde_json::Value>("/favorite/delete", &params)
            .await?;
        Ok(())
    }

    /// Ajoute un track à une playlist
    pub async fn add_to_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()> {
        let user_id = self.ensure_authenticated()?;
        debug!(
            "Adding track {} to playlist {} for user {}",
            track_id, playlist_id, user_id
        );

        let params = [("playlist_id", playlist_id), ("track_ids", track_id)];

        self.get::<serde_json::Value>("/playlist/addTracks", &params)
            .await?;
        Ok(())
    }

    /// Récupère la liste des albums de la bibliothèque utilisateur
    ///
    /// Cette méthode nécessite un secret s4 pour signer la requête.
    /// Elle est principalement utilisée pour tester la validité d'un secret.
    ///
    /// Dans le code Python, cette méthode est utilisée par `setSec()` pour
    /// tester chaque secret retourné par le Spoofer.
    ///
    /// # Errors
    ///
    /// Retourne `QobuzError::Configuration` si le secret n'est pas configuré.
    /// Retourne `QobuzError::Unauthorized` si l'utilisateur n'est pas authentifié.
    pub async fn userlib_get_albums(&self) -> Result<FavoritesResponse> {
        use super::signing;

        // Vérifier l'authentification
        self.ensure_authenticated()?;

        // Vérifier que le secret est disponible
        let secret = self.secret().ok_or_else(|| {
            QobuzError::Configuration(
                "Secret not configured. Cannot sign userLibrary/getAlbumsList request.".to_string(),
            )
        })?;

        let timestamp = signing::get_timestamp();

        // Signer la requête (comme Python: userlib_getAlbums)
        let signature = signing::sign_userlib_get_albums(&timestamp, &secret);

        debug!(
            "Signing userLibrary/getAlbumsList: app_id={}, ts={}",
            self.app_id(),
            timestamp
        );

        // Construire les paramètres signés
        let user_auth_token = self
            .auth_token()
            .ok_or_else(|| QobuzError::Unauthorized("No auth token".to_string()))?;

        let params = [
            ("app_id", self.app_id()),
            ("user_auth_token", user_auth_token.as_str()),
            ("request_ts", timestamp.as_str()),
            ("request_sig", signature.as_str()),
        ];

        // Utiliser POST (comme Python)
        self.post("/userLibrary/getAlbumsList", &params).await
    }

    /// Teste si un secret est valide en essayant de récupérer les albums
    ///
    /// Cette méthode est équivalente au test fait dans `setSec()` en Python.
    /// Elle retourne `true` si le secret fonctionne, `false` sinon.
    pub async fn test_secret(&self, _secret: &[u8]) -> bool {
        // Sauvegarder le secret actuel
        let _current_secret = self.secret();

        // Définir temporairement le nouveau secret
        // Note: cette méthode nécessite &mut self, donc on doit la rendre mutable
        // Pour l'instant, on ne peut pas modifier self dans cette méthode
        // TODO: Refactoriser pour permettre de tester les secrets

        // Restaurer le secret original
        false
    }
}
