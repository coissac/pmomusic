use crate::handler::{ResolvedContent, ResolvedTrack, UrlResolver, UrlResolverError};
use async_trait::async_trait;
use pmodidl::{Container, Item, Resource};
use pmosource::api::get_source as get_source_from_registry;
use pmosource::{BrowseResult, MusicSource, MusicSourceError, SearchQuery, SourceCapabilities};
use std::time::SystemTime;

const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/url-source.webp");

#[derive(Debug)]
pub struct UrlSource {
    resolver: UrlResolver,
}

impl UrlSource {
    pub fn new(resolver: UrlResolver) -> Self {
        Self { resolver }
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
            _ => Err(MusicSourceError::ObjectNotFound(object_id.to_string())),
        }
    }

    /// Résout une URL collée dans la barre de recherche.
    ///
    /// Le `query.text` est l'URL brute saisie par l'utilisateur.
    /// Retourne un stub container dont l'ID correspond au container_id
    /// de la source cible (ex. `qobuz:album:l46fxnqnxp5vs`). Le content
    /// directory handler route le browse() ultérieur vers la bonne source.
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
                // Browsons la source cible pour récupérer les métadonnées réelles
                // (titre album, artiste, pochette, nombre de pistes).
                // On retourne un Container enrichi avec :
                //   - parent_id = source_id  → le frontend route le browse vers la bonne source
                //   - child_count réel        → le frontend sait que le container a du contenu
                //   - titre/artiste/cover     → affichage correct dans l'UI
                // Le container reste jouable comme unité (attach_queue) et navigable.
                if let Some(source) = get_source_from_registry(&source_id).await {
                    match source.browse(&container_id).await {
                        Ok(BrowseResult::Items(tracks)) if !tracks.is_empty() => {
                            let first = tracks.first();
                            let title = first
                                .and_then(|t| t.album.as_deref())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| display_title_for_url(url));
                            let artist = first.and_then(|t| t.artist.clone());
                            let album_art = first.and_then(|t| t.album_art.clone());
                            let container = Container {
                                id: container_id,
                                parent_id: source_id,
                                restricted: Some("1".to_string()),
                                child_count: Some(tracks.len().to_string()),
                                searchable: Some("0".to_string()),
                                title,
                                class: "object.container".to_string(),
                                artist,
                                album_art,
                                containers: vec![],
                                items: vec![],
                            };
                            return Ok(BrowseResult::Containers(vec![container]));
                        }
                        Ok(result) => return Ok(result),
                        Err(e) => {
                            tracing::warn!(
                                source_id = %source_id,
                                container_id = %container_id,
                                error = %e,
                                "UrlSource: browse de la source cible échoué, fallback stub"
                            );
                        }
                    }
                }
                // Fallback : stub minimaliste si la source n'est pas disponible
                let title = display_title_for_url(url);
                let container = Container {
                    id: container_id,
                    parent_id: source_id,
                    restricted: Some("1".to_string()),
                    child_count: None,
                    searchable: Some("0".to_string()),
                    title,
                    class: "object.container".to_string(),
                    artist: None,
                    album_art: None,
                    containers: vec![],
                    items: vec![],
                };
                Ok(BrowseResult::Containers(vec![container]))
            }

            Ok(ResolvedContent::Playlist { title: album, items }) => {
                let album = album.or_else(|| Some(display_title_for_url(url)));
                let didl_items: Vec<Item> = items
                    .into_iter()
                    .enumerate()
                    .map(|(i, t)| resolved_track_to_item(t, i, album.as_deref()))
                    .collect();
                Ok(BrowseResult::Items(didl_items))
            }

            Ok(ResolvedContent::Track(t)) => {
                let item = resolved_track_to_item(t, 0, None);
                Ok(BrowseResult::Items(vec![item]))
            }

            Ok(ResolvedContent::Stream { uri, title, mime_type }) => {
                let item = stream_to_item(uri, title, mime_type);
                Ok(BrowseResult::Items(vec![item]))
            }

            Err(UrlResolverError::NotSupported(_)) => {
                // Texte libre (pas une URL) — les autres sources traitent normalement.
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

/// Convertit un `ResolvedTrack` en `pmodidl::Item` jouable.
fn resolved_track_to_item(t: ResolvedTrack, index: usize, album: Option<&str>) -> Item {
    let protocol_info = format!("http-get:*:{}:*", t.mime_type);
    Item {
        id: format!("url:item:{}", index),
        parent_id: "url".to_string(),
        restricted: Some("1".to_string()),
        title: t.title,
        creator: t.artist.clone(),
        class: "object.item.audioItem.musicTrack".to_string(),
        artist: t.artist,
        album: t.album.or_else(|| album.map(|s| s.to_string())),
        genre: None,
        album_art: t.album_art,
        album_art_pk: None,
        date: None,
        original_track_number: Some(format!("{}", index + 1)),
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
/// Ex: "https://open.qobuz.com/album/abc" → "Album (open.qobuz.com)"
fn display_title_for_url(url: &str) -> String {
    // Extraire l'hôte
    let host = url
        .find("://")
        .and_then(|i| {
            let after = &url[i + 3..];
            let end = after.find('/').unwrap_or(after.len());
            Some(&after[..end])
        })
        .unwrap_or("");

    // Extraire le premier segment du path
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
