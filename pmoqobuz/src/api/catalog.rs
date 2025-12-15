//! Module d'accès au catalogue Qobuz (albums, tracks, artistes, playlists)

use super::QobuzApi;
use crate::error::{QobuzError, Result};
use crate::models::*;
use serde::Deserialize;
use tracing::debug;

/// Réponse paginée de l'API
#[derive(Debug, Deserialize)]
struct PaginatedResponse<T> {
    items: Vec<T>,
    #[serde(default)]
    total: Option<u32>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

/// Réponse de l'endpoint /album/get
#[derive(Debug, Deserialize)]
pub(crate) struct AlbumResponse {
    #[serde(deserialize_with = "crate::models::deserialize_id")]
    id: String,
    title: String,
    artist: ArtistResponse,
    #[serde(default)]
    tracks_count: Option<u32>,
    #[serde(default)]
    duration: Option<u32>,
    #[serde(default)]
    release_date_original: Option<String>,
    #[serde(default)]
    image: Option<ImageResponse>,
    #[serde(default = "default_streamable")]
    streamable: bool,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    maximum_sampling_rate: Option<f64>,
    #[serde(default)]
    maximum_bit_depth: Option<u32>,
    #[serde(default)]
    genre: Option<GenreResponse>,
    #[serde(default)]
    label: Option<LabelResponse>,
    #[serde(default)]
    tracks: Option<PaginatedResponse<TrackResponse>>,
}

/// Réponse de l'endpoint /track/get
#[derive(Debug, Deserialize)]
pub(crate) struct TrackResponse {
    #[serde(deserialize_with = "crate::models::deserialize_id")]
    id: String,
    title: String,
    #[serde(default)]
    performer: Option<ArtistResponse>,
    #[serde(default)]
    artist: Option<ArtistResponse>,
    #[serde(default)]
    album: Option<AlbumResponse>,
    duration: u32,
    track_number: u32,
    media_number: u32,
    #[serde(default = "default_streamable")]
    streamable: bool,
}

/// Réponse artiste
#[derive(Debug, Deserialize)]
pub(crate) struct ArtistResponse {
    #[serde(deserialize_with = "crate::models::deserialize_id")]
    id: String,
    name: String,
    #[serde(default)]
    image: Option<ImageResponse>,
    #[serde(default)]
    albums: Option<PaginatedResponse<AlbumResponse>>,
}

/// Réponse image
#[derive(Debug, Deserialize)]
struct ImageResponse {
    #[serde(default)]
    large: Option<String>,
}

/// Réponse genre
#[derive(Debug, Deserialize)]
struct GenreResponse {
    #[serde(default)]
    id: Option<u32>,
    name: String,
}

/// Réponse label
#[derive(Debug, Deserialize)]
struct LabelResponse {
    name: String,
}

/// Réponse playlist
#[derive(Debug, Deserialize)]
pub(crate) struct PlaylistResponse {
    #[serde(deserialize_with = "crate::models::deserialize_id")]
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tracks_count: Option<u32>,
    #[serde(default)]
    duration: Option<u32>,
    #[serde(default)]
    images300: Option<Vec<String>>,
    #[serde(default)]
    is_public: bool,
    #[serde(default)]
    owner: Option<OwnerResponse>,
    #[serde(default)]
    tracks: Option<PaginatedResponse<TrackResponse>>,
}

/// Réponse propriétaire
#[derive(Debug, Deserialize)]
struct OwnerResponse {
    #[serde(deserialize_with = "crate::models::deserialize_id")]
    id: String,
    name: String,
}

/// Réponse genres list
#[derive(Debug, Deserialize)]
struct GenresResponse {
    genres: PaginatedResponse<GenreResponse>,
}

/// Réponse albums featured
#[derive(Debug, Deserialize)]
struct FeaturedAlbumsResponse {
    albums: PaginatedResponse<AlbumResponse>,
}

/// Réponse playlists featured
#[derive(Debug, Deserialize)]
struct FeaturedPlaylistsResponse {
    playlists: PaginatedResponse<PlaylistResponse>,
}

/// Réponse search
#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    albums: Option<PaginatedResponse<AlbumResponse>>,
    #[serde(default)]
    artists: Option<PaginatedResponse<ArtistResponse>>,
    #[serde(default)]
    tracks: Option<PaginatedResponse<TrackResponse>>,
    #[serde(default)]
    playlists: Option<PaginatedResponse<PlaylistResponse>>,
}

/// Réponse track file URL
#[derive(Debug, Deserialize)]
struct FileUrlResponse {
    url: String,
    mime_type: String,
    sampling_rate: f64,
    bit_depth: u32,
    format_id: u8,
}

fn default_streamable() -> bool {
    true
}

impl QobuzApi {
    /// Récupère les détails d'un album
    pub async fn get_album(&self, album_id: &str) -> Result<Album> {
        debug!("Fetching album {}", album_id);
        let params = [("album_id", album_id)];
        let response: AlbumResponse = self.get("/album/get", &params).await?;
        Ok(Self::parse_album(response))
    }

    /// Récupère les tracks d'un album
    pub async fn get_album_tracks(&self, album_id: &str) -> Result<Vec<Track>> {
        debug!("Fetching tracks for album {}", album_id);
        let params = [("album_id", album_id)];
        let mut response: AlbumResponse = self.get("/album/get", &params).await?;

        if let Some(tracks) = response.tracks.take() {
            let album = Self::parse_album(response);
            Ok(tracks
                .items
                .into_iter()
                .map(|t| Self::parse_track(t, Some(album.clone())))
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Récupère les détails d'une track
    pub async fn get_track(&self, track_id: &str) -> Result<Track> {
        debug!("Fetching track {}", track_id);
        let params = [("track_id", track_id)];
        let response: TrackResponse = self.get("/track/get", &params).await?;
        Ok(Self::parse_track(response, None))
    }

    /// Récupère l'URL de streaming d'une track
    ///
    /// Cette méthode nécessite un secret s4 pour signer la requête.
    /// Si aucun secret n'est configuré, retourne une erreur.
    ///
    /// # Errors
    ///
    /// Retourne `QobuzError::Configuration` si le secret n'est pas configuré.
    pub async fn get_file_url(&self, track_id: &str) -> Result<StreamInfo> {
        use super::signing;

        debug!("Fetching file URL for track {}", track_id);

        // Vérifier que le secret est disponible
        let secret = self.secret().ok_or_else(|| {
            QobuzError::Configuration(
                "Secret not configured. Cannot sign track/getFileUrl request.".to_string(),
            )
        })?;

        let format_id = self.format_id.id().to_string();
        let intent = "stream";
        let timestamp = signing::get_timestamp();

        // Vérifier que le token d'authentification est disponible
        self.auth_token()
            .ok_or_else(|| QobuzError::Unauthorized("Missing auth token".to_string()))?;

        // Signer la requête (comme Python: track_getFileUrl)
        let signature =
            signing::sign_track_get_file_url(&format_id, intent, track_id, &timestamp, &secret);

        debug!(
            "Signing track/getFileUrl: track_id={}, format_id={}, ts={}",
            track_id, format_id, timestamp
        );

        // Construire les paramètres signés
        // Note: app_id et user_auth_token sont envoyés automatiquement comme headers
        // par la méthode request() (X-App-Id et X-User-Auth-Token)
        // IMPORTANT: L'ordre doit correspondre EXACTEMENT à Python (raw.py)
        let params = [
            ("format_id", format_id.as_str()),
            ("intent", intent),
            ("request_ts", timestamp.as_str()),
            ("request_sig", signature.as_str()),
            ("track_id", track_id),
        ];

        // Utiliser GET (comme qobuz-player-client qui fonctionne)
        let response: FileUrlResponse = self.get("/track/getFileUrl", &params).await?;

        Ok(StreamInfo {
            url: response.url,
            mime_type: response.mime_type,
            sampling_rate: response.sampling_rate,
            bit_depth: response.bit_depth,
            format_id: response.format_id,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
        })
    }

    /// Récupère les albums d'un artiste
    pub async fn get_artist_albums(&self, artist_id: &str) -> Result<Vec<Album>> {
        debug!("Fetching albums for artist {}", artist_id);
        let params = [("artist_id", artist_id), ("extra", "albums")];
        let response: ArtistResponse = self.get("/artist/get", &params).await?;

        if let Some(albums) = response.albums {
            Ok(albums
                .items
                .into_iter()
                .map(Self::parse_album)
                .filter(|a| a.streamable)
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Récupère les artistes similaires
    pub async fn get_similar_artists(&self, artist_id: &str) -> Result<Vec<Artist>> {
        debug!("Fetching similar artists for {}", artist_id);
        let params = [("artist_id", artist_id)];

        #[derive(Debug, Deserialize)]
        struct SimilarArtistsResponse {
            artists: PaginatedResponse<ArtistResponse>,
        }

        let response: SimilarArtistsResponse =
            self.get("/artist/getSimilarArtists", &params).await?;
        Ok(response
            .artists
            .items
            .into_iter()
            .map(Self::parse_artist)
            .collect())
    }

    /// Récupère les détails d'une playlist
    pub async fn get_playlist(&self, playlist_id: &str) -> Result<Playlist> {
        debug!("Fetching playlist {}", playlist_id);
        let params = [("playlist_id", playlist_id)];
        let response: PlaylistResponse = self.get("/playlist/get", &params).await?;
        Ok(Self::parse_playlist(response))
    }

    /// Récupère les tracks d'une playlist
    pub async fn get_playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        debug!("Fetching tracks for playlist {}", playlist_id);
        let params = [("playlist_id", playlist_id), ("extra", "tracks")];
        let response: PlaylistResponse = self.get("/playlist/get", &params).await?;

        if let Some(tracks) = response.tracks {
            Ok(tracks
                .items
                .into_iter()
                .map(|t| Self::parse_track(t, None))
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Récupère la liste des genres
    pub async fn get_genres(&self) -> Result<Vec<Genre>> {
        debug!("Fetching genres");
        let response: GenresResponse = self.get("/genre/list", &[]).await?;
        Ok(response
            .genres
            .items
            .into_iter()
            .map(Self::parse_genre)
            .collect())
    }

    /// Récupère les albums featured (nouveautés, éditeur, etc.)
    pub async fn get_featured_albums(
        &self,
        genre_id: Option<&str>,
        type_: &str,
    ) -> Result<Vec<Album>> {
        debug!("Fetching featured albums (type: {})", type_);
        let mut params = vec![("type", type_), ("limit", "100")];

        if let Some(gid) = genre_id {
            params.push(("genre_ids", gid));
        }

        let response: FeaturedAlbumsResponse = self.get("/album/getFeatured", &params).await?;
        Ok(response
            .albums
            .items
            .into_iter()
            .map(Self::parse_album)
            .filter(|a| a.streamable)
            .collect())
    }

    /// Récupère les playlists featured
    pub async fn get_featured_playlists(
        &self,
        genre_id: Option<&str>,
        tags: Option<&str>,
    ) -> Result<Vec<Playlist>> {
        debug!("Fetching featured playlists");
        let mut params = vec![("type", "editor-picks"), ("limit", "100")];

        if let Some(gid) = genre_id {
            params.push(("genre_ids", gid));
        }
        if let Some(t) = tags {
            params.push(("tags", t));
        }

        let response: FeaturedPlaylistsResponse =
            self.get("/playlist/getFeatured", &params).await?;
        Ok(response
            .playlists
            .items
            .into_iter()
            .map(Self::parse_playlist)
            .collect())
    }

    /// Recherche dans le catalogue
    pub async fn search(&self, query: &str, type_: Option<&str>) -> Result<SearchResult> {
        debug!("Searching for '{}' (type: {:?})", query, type_);
        let mut params = vec![("query", query), ("limit", "200")];

        if let Some(t) = type_ {
            params.push(("type", t));
        }

        let response: SearchResponse = self.get("/catalog/search", &params).await?;

        Ok(SearchResult {
            albums: response
                .albums
                .map(|a| {
                    a.items
                        .into_iter()
                        .map(Self::parse_album)
                        .filter(|album| album.streamable)
                        .collect()
                })
                .unwrap_or_default(),
            artists: response
                .artists
                .map(|a| a.items.into_iter().map(Self::parse_artist).collect())
                .unwrap_or_default(),
            tracks: response
                .tracks
                .map(|t| {
                    t.items
                        .into_iter()
                        .map(|track| Self::parse_track(track, None))
                        .filter(|track| track.streamable)
                        .collect()
                })
                .unwrap_or_default(),
            playlists: response
                .playlists
                .map(|p| p.items.into_iter().map(Self::parse_playlist).collect())
                .unwrap_or_default(),
        })
    }

    // Fonctions de parsing publiques (utilisées aussi par le module user)

    pub(crate) fn parse_album(response: AlbumResponse) -> Album {
        Album {
            id: response.id,
            title: response.title,
            artist: Self::parse_artist(response.artist),
            tracks_count: response.tracks_count,
            duration: response.duration,
            release_date: response.release_date_original,
            image: response.image.and_then(|i| i.large),
            image_cached: None,
            streamable: response.streamable,
            description: response.description,
            maximum_sampling_rate: response.maximum_sampling_rate,
            maximum_bit_depth: response.maximum_bit_depth,
            genres: response.genre.map(|g| vec![g.name]).unwrap_or_default(),
            label: response.label.map(|l| l.name),
        }
    }

    pub(crate) fn parse_track(response: TrackResponse, album: Option<Album>) -> Track {
        let performer = response
            .performer
            .or(response.artist)
            .map(Self::parse_artist);

        let album = album.or_else(|| response.album.map(Self::parse_album));

        Track {
            id: response.id,
            title: response.title,
            performer,
            album,
            duration: response.duration,
            track_number: response.track_number,
            media_number: response.media_number,
            streamable: response.streamable,
            mime_type: None,
            sample_rate: None,
            bit_depth: None,
            channels: None,
        }
    }

    pub(crate) fn parse_artist(response: ArtistResponse) -> Artist {
        Artist {
            id: response.id,
            name: response.name,
            image: response.image.and_then(|i| i.large),
            image_cached: None,
        }
    }

    pub(crate) fn parse_playlist(response: PlaylistResponse) -> Playlist {
        Playlist {
            id: response.id,
            name: response.name,
            description: response.description,
            tracks_count: response.tracks_count,
            duration: response.duration,
            image: response.images300.and_then(|imgs| imgs.first().cloned()),
            image_cached: None,
            is_public: response.is_public,
            owner: response.owner.map(|o| PlaylistOwner {
                id: o.id.parse().unwrap_or(0),
                name: o.name,
            }),
        }
    }

    pub(crate) fn parse_genre(response: GenreResponse) -> Genre {
        Genre {
            id: response.id,
            name: response.name,
            children: Vec::new(),
        }
    }
}
