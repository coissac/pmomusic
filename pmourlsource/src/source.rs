use crate::handler::{ResolvedContent, ResolvedTrack, UrlResolver, UrlResolverError};
use async_trait::async_trait;
use pmodidl::{Container, Item, Resource};
use pmosource::api::get_source as get_source_from_registry;
use pmosource::{BrowseResult, MusicSource, MusicSourceError, SearchQuery, SourceCapabilities};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::RwLock;
use std::sync::Arc;
use std::time::SystemTime;

const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/url-source.webp");

/// Store éphémère pour les playlists URL : playlist_id → (container, items)
type PlaylistStore = Arc<RwLock<HashMap<String, (Container, Vec<Item>)>>>;

#[derive(Debug)]
pub struct UrlSource {
    resolver: UrlResolver,
    base_url: String,
    playlists: PlaylistStore,
}

impl UrlSource {
    pub fn new(resolver: UrlResolver, base_url: String) -> Self {
        Self {
            resolver,
            base_url,
            playlists: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl MusicSource for UrlSource {
    fn name(&self) -> &str {
        "URL / Partage"
    }

    fn id(&self) -> &str {
        "url"
    }

    fn default_image(&self) -> &[u8] {
        DEFAULT_IMAGE
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            supports_search: true,
            handles_url_input: true,
            ..Default::default()
        }
    }

    async fn root_container(&self) -> pmosource::Result<Container> {
        Ok(Container {
            id: "url".to_string(),
            parent_id: "0".to_string(),
            restricted: Some("1".to_string()),
            child_count: None,
            searchable: Some("1".to_string()),
            title: "URL / Partage".to_string(),
            class: "object.container".to_string(),
            artist: None,
            album_art: None,
            containers: vec![],
            items: vec![],
        })
    }

    async fn browse(&self, object_id: &str) -> pmosource::Result<BrowseResult> {
        match object_id {
            "url" => Ok(BrowseResult::Containers(vec![])),
            // Playlist éphémère créée par build_url_playlist
            _ if object_id.starts_with("urlsource-") => {
                let store = self.playlists.read().map_err(|_| {
                    MusicSourceError::BrowseError("playlist store lock poisoned".to_string())
                })?;
                match store.get(object_id) {
                    Some((_, items)) => Ok(BrowseResult::Items(items.clone())),
                    None => Err(MusicSourceError::ObjectNotFound(object_id.to_string())),
                }
            }
            // Court-circuiter les IDs "url:*" pour éviter des erreurs dans les logs
            // des autres sources (items éphémères non persistables par ID).
            _ if object_id.starts_with("url:") => {
                Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
            }
            _ => Err(MusicSourceError::ObjectNotFound(object_id.to_string())),
        }
    }

    async fn search(&self, query: &SearchQuery) -> pmosource::Result<BrowseResult> {
        let url = query.text.trim();

        if url.is_empty() {
            return Ok(BrowseResult::Containers(vec![]));
        }

        match self.resolver.resolve(url).await {
            Ok(ResolvedContent::SourceContainer {
                source_id,
                container_id,
            }) => {
                if let Some(source) = get_source_from_registry(&source_id).await {
                    match source.get_container(&container_id).await {
                        Ok(Some(mut container)) => {
                            container.parent_id = source_id;
                            return Ok(BrowseResult::Containers(vec![container]));
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::warn!(
                                source_id = %source_id,
                                container_id = %container_id,
                                error = %e,
                                "UrlSource: get_container échoué"
                            );
                        }
                    }
                    match source.get_item(&container_id).await {
                        Ok(item) => return Ok(BrowseResult::Items(vec![item])),
                        Err(_) => {}
                    }
                }
                let title = display_title_for_url(url);
                let container = Container {
                    id: container_id,
                    parent_id: source_id,
                    restricted: Some("1".to_string()),
                    child_count: Some("1".to_string()),
                    searchable: Some("1".to_string()),
                    title,
                    class: "object.container".to_string(),
                    artist: None,
                    album_art: None,
                    containers: vec![],
                    items: vec![],
                };
                Ok(BrowseResult::Containers(vec![container]))
            }

            Ok(ResolvedContent::Playlist { title: playlist_title, items }) => {
                let title = playlist_title.unwrap_or_else(|| display_title_for_url(url));
                let container = self.build_url_playlist(url, title, items);
                Ok(BrowseResult::Containers(vec![container]))
            }

            Ok(ResolvedContent::Track(t)) => {
                // Pour un épisode unique, créer une playlist avec 1 item.
                // Titre de la playlist = nom du podcast (album) ou titre de l'épisode.
                let title = t.album.clone()
                    .or_else(|| Some(t.title.clone()))
                    .unwrap_or_else(|| display_title_for_url(url));
                let container = self.build_url_playlist(url, title, vec![t]);
                Ok(BrowseResult::Containers(vec![container]))
            }

            Ok(ResolvedContent::Stream { uri, title, mime_type }) => {
                let item = stream_to_item(uri, title, mime_type);
                Ok(BrowseResult::Items(vec![item]))
            }

            Err(UrlResolverError::NotSupported(_)) => {
                Ok(BrowseResult::Containers(vec![]))
            }
            Err(e) => {
                tracing::warn!(url = %url, error = %e, "UrlSource: résolution échouée");
                Err(MusicSourceError::BrowseError(format!(
                    "Résolution URL échouée : {}",
                    e
                )))
            }
        }
    }

    async fn resolve_uri(&self, object_id: &str) -> pmosource::Result<String> {
        Err(MusicSourceError::ObjectNotFound(object_id.to_string()))
    }

    fn supports_fifo(&self) -> bool {
        false
    }

    async fn append_track(&self, _track: Item) -> pmosource::Result<()> {
        Err(MusicSourceError::FifoNotSupported)
    }

    async fn remove_oldest(&self) -> pmosource::Result<Option<Item>> {
        Err(MusicSourceError::FifoNotSupported)
    }

    async fn update_id(&self) -> u32 {
        1
    }

    async fn last_change(&self) -> Option<SystemTime> {
        None
    }

    async fn get_items(&self, _offset: usize, _count: usize) -> pmosource::Result<Vec<Item>> {
        Ok(vec![])
    }
}

impl UrlSource {
    /// Crée un container playlist éphémère en mémoire depuis des tracks résolus.
    ///
    /// Les items gardent leurs URLs directes (RadioFrance, etc.) et leur MIME type
    /// d'origine — pas de proxy via pmoaudiocache, donc pas de conversion FLAC
    /// et pas de problème avec les formats M4A/AAC.
    fn build_url_playlist(&self, url: &str, title: String, tracks: Vec<ResolvedTrack>) -> Container {
        let playlist_id = format!("urlsource-{:016x}", url_hash(url));
        let n = tracks.len();

        // Cover = album_art du premier épisode
        let album_art = tracks.first().and_then(|t| t.album_art.clone());

        let items: Vec<Item> = tracks
            .into_iter()
            .enumerate()
            .map(|(i, t)| {
                let protocol_info = format!("http-get:*:{}:*", t.mime_type);
                Item {
                    id: format!("{}:{}", playlist_id, i),
                    parent_id: playlist_id.clone(),
                    restricted: Some("1".to_string()),
                    title: t.title,
                    creator: t.artist.clone(),
                    class: "object.item.audioItem.musicTrack".to_string(),
                    artist: t.artist,
                    album: t.album.or_else(|| Some(title.clone())),
                    genre: None,
                    album_art: t.album_art,
                    album_art_pk: None,
                    date: None,
                    original_track_number: Some(format!("{}", i + 1)),
                    resources: vec![Resource {
                        protocol_info,
                        bits_per_sample: None,
                        sample_frequency: None,
                        nr_audio_channels: None,
                        duration: t.duration,
                        url: t.uri,
                    }],
                    descriptions: vec![],
                }
            })
            .collect();

        let container = Container {
            id: playlist_id.clone(),
            parent_id: "url".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some(n.to_string()),
            searchable: Some("0".to_string()),
            title: title.clone(),
            class: "object.container.playlistContainer".to_string(),
            artist: None,
            album_art,
            containers: vec![],
            items: vec![],
        };

        // Stocker dans le store éphémère (écrase toute entrée précédente)
        if let Ok(mut store) = self.playlists.write() {
            store.insert(playlist_id, (container.clone(), items));
        }

        container
    }
}

/// Convertit un flux continu en `pmodidl::Item`.
fn stream_to_item(uri: String, title: String, mime_type: String) -> Item {
    let protocol_info = format!("http-get:*:{}:*", mime_type);
    Item {
        id: "url:item:0".to_string(),
        parent_id: "url".to_string(),
        restricted: Some("1".to_string()),
        title,
        creator: None,
        class: "object.item.audioItem.audioBroadcast".to_string(),
        artist: None,
        album: None,
        genre: None,
        album_art: None,
        album_art_pk: None,
        date: None,
        original_track_number: None,
        resources: vec![Resource {
            protocol_info,
            bits_per_sample: None,
            sample_frequency: None,
            nr_audio_channels: None,
            duration: None,
            url: uri,
        }],
        descriptions: vec![],
    }
}

/// Extrait un titre lisible depuis une URL.
fn display_title_for_url(url: &str) -> String {
    let host = url
        .find("://")
        .and_then(|i| {
            let after = &url[i + 3..];
            let end = after.find('/').unwrap_or(after.len());
            Some(&after[..end])
        })
        .unwrap_or("");

    let type_label = if url.contains("/album/") {
        "Album"
    } else if url.contains("/track/") {
        "Titre"
    } else if url.contains("/playlist/") {
        "Playlist"
    } else if url.contains("/artist/") {
        "Artiste"
    } else {
        "Contenu"
    };

    if host.is_empty() {
        type_label.to_string()
    } else {
        format!("{} ({})", type_label, host)
    }
}

/// Hash stable d'une URL pour construire un ID de playlist déterministe.
fn url_hash(url: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    hasher.finish()
}
