use crate::handler::{ResolvedContent, ResolvedTrack, UrlHandler, UrlResolverError};
use async_trait::async_trait;
use futures::future::join_all;
use reqwest::{redirect, Client};
use std::sync::Arc;

/// Handler dédié aux URLs radiofrance.fr — priorité 80.
///
/// RadioFrance utilise SvelteKit (SSR).
///
/// Stratégie selon le type d'URL :
///
/// 1. **Page podcast** (`/podcasts/{slug}`)
///    → `rssFeed:"https://..."` inline → fetch + parse RSS
///
/// 2. **Page série** (`/podcasts/serie-{slug}`)
///    → JSON-LD `ItemList` → extraire les URLs d'épisodes → fetch concurrent
///    (RadioFrance limite leur RSS à 2 éléments ; scraping direct donne tous les épisodes)
///
/// 3. **Page épisode** (`/podcasts/{podcast}/{episode}-{id}`)
///    → URL MP3 `media.radiofrance-podcast.net` inline
pub struct RadioFranceUrlHandler {
    client: Arc<Client>,
}

impl RadioFranceUrlHandler {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .redirect(redirect::Policy::limited(5))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(20))
            .build()?;
        Ok(Self { client: Arc::new(client) })
    }

    async fn resolve_inner(&self, url: &str) -> Result<ResolvedContent, UrlResolverError> {
        let html = self.fetch_html(url).await?;

        // --- Cas 1 : page podcast → rssFeed non vide ---
        if let Some(rss_url) = extract_rss_feed_key(&html) {
            tracing::debug!(rss_url = %rss_url, "RadioFrance: rssFeed trouvé");
            return self.fetch_rss(&rss_url).await;
        }

        // --- Cas 2 : page série → fetch concurrent des pages épisodes ---
        let episode_urls = extract_episode_urls_from_series(&html, url);
        if !episode_urls.is_empty() {
            tracing::debug!(
                count = episode_urls.len(),
                "RadioFrance: série — fetch concurrent des épisodes"
            );
            let feed_title = extract_og_title(&html);
            let feed_image = extract_og_image(&html);
            let album = feed_title.clone().or_else(|| extract_title_tag(&html));

            let client = self.client.clone();
            let fetches: Vec<_> = episode_urls
                .into_iter()
                .map(|ep_url| {
                    let client = client.clone();
                    let album = album.clone();
                    let feed_image = feed_image.clone();
                    async move {
                        match fetch_html_with_client(&client, &ep_url).await {
                            Ok(ep_html) => episode_to_track(&ep_html, &ep_url, album.as_deref(), feed_image.as_deref()),
                            Err(_) => None,
                        }
                    }
                })
                .collect();

            let tracks: Vec<ResolvedTrack> = join_all(fetches).await.into_iter().flatten().collect();

            if !tracks.is_empty() {
                return Ok(ResolvedContent::Playlist {
                    title: feed_title,
                    items: tracks,
                });
            }
        }

        // --- Cas 3 : page épisode → MP3 direct ---
        if let Some(mp3_url) = extract_mp3_url(&html) {
            tracing::debug!(mp3_url = %mp3_url, "RadioFrance: MP3 direct trouvé");
            let title = extract_og_title(&html)
                .or_else(|| extract_title_tag(&html))
                .unwrap_or_else(|| url.to_string());
            return Ok(ResolvedContent::Track(ResolvedTrack {
                uri: mp3_url,
                title,
                artist: None,
                album: None,
                duration: None,
                album_art: extract_og_image(&html),
                mime_type: "audio/mpeg".to_string(),
            }));
        }

        Err(UrlResolverError::NotSupported(format!(
            "Aucun podcast/épisode trouvé sur la page RadioFrance : {}",
            url
        )))
    }

    async fn fetch_html(&self, url: &str) -> Result<String, UrlResolverError> {
        fetch_html_with_client(&self.client, url).await
    }

    async fn fetch_rss(&self, rss_url: &str) -> Result<ResolvedContent, UrlResolverError> {
        let body = self
            .client
            .get(rss_url)
            .send()
            .await
            .map_err(|e| UrlResolverError::ResolutionFailed(format!("RSS fetch : {}", e)))?
            .text()
            .await
            .map_err(|e| UrlResolverError::ResolutionFailed(e.to_string()))?;

        parse_rss(&body, rss_url)
    }
}

impl Default for RadioFranceUrlHandler {
    fn default() -> Self {
        Self::new().expect("Failed to build HTTP client for RadioFranceUrlHandler")
    }
}

#[async_trait]
impl UrlHandler for RadioFranceUrlHandler {
    fn name(&self) -> &str {
        "RadioFranceUrlHandler"
    }

    fn priority(&self) -> u8 {
        80
    }

    fn can_handle(&self, url: &str) -> bool {
        url.contains("radiofrance.fr")
    }

    async fn resolve(&self, url: &str) -> Result<ResolvedContent, UrlResolverError> {
        self.resolve_inner(url).await
    }
}

// ── HTTP helpers ─────────────────────────────────────────────────────────────

async fn fetch_html_with_client(client: &Client, url: &str) -> Result<String, UrlResolverError> {
    client
        .get(url)
        .header("Accept", "text/html,application/xhtml+xml")
        .header("Accept-Language", "fr-FR,fr;q=0.9")
        .send()
        .await
        .map_err(|e| UrlResolverError::ResolutionFailed(e.to_string()))?
        .text()
        .await
        .map_err(|e| UrlResolverError::ResolutionFailed(e.to_string()))
}

// ── Extraction SvelteKit ─────────────────────────────────────────────────────

/// Cherche `rssFeed:"https://..."` dans le JS SvelteKit inline.
/// Retourne None si le champ est absent ou vide.
fn extract_rss_feed_key(html: &str) -> Option<String> {
    let needle = "rssFeed:\"https://";
    let pos = html.find(needle)?;
    let start = pos + "rssFeed:\"".len();
    let end = html[start..].find('"')? + start;
    let url = html[start..end].replace("\\/", "/");
    if url.is_empty() || !url.starts_with("http") {
        None
    } else {
        Some(url)
    }
}

/// Extrait toutes les URLs d'épisodes depuis le JSON-LD `ItemList` d'une page série.
///
/// Filtre les URLs non-épisodes (série elle-même, images, domaine seul…).
/// Une URL d'épisode a exactement 4 segments de path :
///   `/{station}/podcasts/{podcast-slug}/{episode-slug}`
fn extract_episode_urls_from_series(html: &str, series_url: &str) -> Vec<String> {
    let item_marker = "\"@type\":\"ItemList\"";
    let list_pos = match html.find(item_marker) {
        Some(p) => p,
        None => return vec![],
    };

    let url_prefix = "\"url\":\"https://www.radiofrance.fr/";
    let after_list = &html[list_pos..];
    let mut urls = Vec::new();
    let mut search_from = 0;

    while let Some(rel_pos) = after_list[search_from..].find(url_prefix) {
        let rel_pos = search_from + rel_pos;
        let from = list_pos + rel_pos + "\"url\":\"".len();
        let Some(end_rel) = html[from..].find('"') else { break };
        let candidate = &html[from..from + end_rel];

        if is_episode_url(candidate, series_url) {
            urls.push(candidate.to_string());
        }
        search_from = rel_pos + url_prefix.len();
    }
    urls
}

/// Retourne true si l'URL est bien une page d'épisode (≥4 segments de path).
fn is_episode_url(url: &str, series_url: &str) -> bool {
    if url.trim_end_matches('/') == series_url.trim_end_matches('/') {
        return false;
    }
    if !url.contains("/podcasts/") {
        return false;
    }
    // Exclure fichiers statiques (images…)
    let last = url.rsplit('/').next().unwrap_or("");
    if last.contains('.') {
        return false;
    }
    // Doit avoir ≥ 4 segments après le domaine : /station/podcasts/podcast/episode
    let path_segments: usize = url
        .splitn(4, "radiofrance.fr")
        .nth(1)
        .unwrap_or("")
        .split('/')
        .filter(|s| !s.is_empty())
        .count();
    path_segments >= 4
}

/// Extrait un `ResolvedTrack` depuis la page HTML d'un épisode RadioFrance.
fn episode_to_track(html: &str, url: &str, album: Option<&str>, feed_image: Option<&str>) -> Option<ResolvedTrack> {
    let mp3_url = extract_mp3_url(html)?;
    let title = extract_og_title(html)
        .or_else(|| extract_title_tag(html))
        .unwrap_or_else(|| url.to_string());
    let album_art = extract_og_image(html).or_else(|| feed_image.map(|s| s.to_string()));
    Some(ResolvedTrack {
        uri: mp3_url,
        title,
        artist: None,
        album: album.map(|s| s.to_string()),
        duration: None,
        album_art,
        mime_type: "audio/mpeg".to_string(),
    })
}

/// Extrait l'URL du premier fichier MP3 hébergé sur media.radiofrance-podcast.net.
fn extract_mp3_url(html: &str) -> Option<String> {
    let needle = "https://media.radiofrance-podcast.net/";
    let pos = html.find(needle)?;
    let end = html[pos..].find(|c: char| c == '"' || c == '\'' || c.is_whitespace())? + pos;
    let url = html[pos..end].to_string();
    if url.ends_with(".mp3") || url.contains(".mp3?") || url.contains("ITEMA_") {
        Some(url)
    } else {
        None
    }
}

// ── RSS parser ───────────────────────────────────────────────────────────────

fn parse_rss(body: &str, source_url: &str) -> Result<ResolvedContent, UrlResolverError> {
    let mut items: Vec<ResolvedTrack> = Vec::new();
    let mut feed_title: Option<String> = None;
    let mut feed_image: Option<String> = None;
    let mut current_title: Option<String> = None;
    let mut current_uri: Option<String> = None;
    let mut current_duration: Option<String> = None;
    let mut current_image: Option<String> = None;
    let mut in_item = false;

    for line in body.lines() {
        let trimmed = line.trim();

        if !in_item {
            if trimmed.starts_with("<title") && feed_title.is_none() {
                feed_title = extract_xml_text(trimmed, "title");
            }
            if trimmed.contains("<itunes:image") || trimmed.contains("<image>") {
                if let Some(href) = extract_attr(trimmed, "href") {
                    feed_image = Some(href);
                }
            }
        }

        if trimmed == "<item>" || trimmed.starts_with("<item ") {
            in_item = true;
            current_title = None;
            current_uri = None;
            current_duration = None;
            current_image = None;
            continue;
        }

        if trimmed == "</item>" {
            if let (Some(uri), Some(title)) = (current_uri.take(), current_title.take()) {
                items.push(ResolvedTrack {
                    uri,
                    title,
                    artist: None,
                    album: feed_title.clone(),
                    duration: current_duration.take().map(itunes_duration_to_upnp),
                    album_art: current_image.take().or_else(|| feed_image.clone()),
                    mime_type: "audio/mpeg".to_string(),
                });
            }
            in_item = false;
            continue;
        }

        if !in_item {
            continue;
        }

        if trimmed.starts_with("<title") && current_title.is_none() {
            current_title = extract_xml_text(trimmed, "title");
        } else if trimmed.starts_with("<enclosure") {
            if let Some(url) = extract_attr(trimmed, "url") {
                let type_ = extract_attr(trimmed, "type").unwrap_or_default();
                if type_.starts_with("audio/") || type_.is_empty() {
                    current_uri = Some(url);
                }
            }
        } else if trimmed.starts_with("<itunes:duration") {
            current_duration = extract_xml_text(trimmed, "itunes:duration");
        } else if trimmed.starts_with("<itunes:image") {
            if let Some(href) = extract_attr(trimmed, "href") {
                current_image = Some(href);
            }
        }
    }

    if items.is_empty() {
        return Err(UrlResolverError::NotSupported(format!(
            "Aucun épisode dans le feed RSS RadioFrance : {}",
            source_url
        )));
    }

    Ok(ResolvedContent::Playlist {
        title: feed_title,
        items,
    })
}

// ── Utilitaires HTML ─────────────────────────────────────────────────────────

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let tag_lower = tag.to_lowercase();
    let attr_lower = attr.to_lowercase();
    let needle = format!("{}=", attr_lower);
    let pos = tag_lower.find(&needle)? + needle.len();
    let rest = &tag[pos..];
    if rest.starts_with('"') {
        let end = rest[1..].find('"')? + 1;
        Some(rest[1..end].to_string())
    } else if rest.starts_with('\'') {
        let end = rest[1..].find('\'')? + 1;
        Some(rest[1..end].to_string())
    } else {
        let end = rest.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

fn extract_xml_text(line: &str, tag: &str) -> Option<String> {
    let open_plain = format!("<{}>", tag);
    let open_with_attrs = format!("<{} ", tag);
    let close = format!("</{}>", tag);
    let content_start = if let Some(p) = line.find(&open_plain) {
        p + open_plain.len()
    } else if let Some(p) = line.find(&open_with_attrs) {
        let after = &line[p..];
        let gt = after.find('>')?;
        p + gt + 1
    } else {
        return None;
    };
    let content_end = line[content_start..].find(&close)? + content_start;
    let text = line[content_start..content_end]
        .trim()
        .replace("<![CDATA[", "")
        .replace("]]>", "");
    if text.is_empty() { None } else { Some(text) }
}

fn extract_og_title(html: &str) -> Option<String> {
    extract_meta_property(html, "og:title")
}

fn extract_og_image(html: &str) -> Option<String> {
    extract_meta_property(html, "og:image")
}

fn extract_title_tag(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")? + 6;
    let start = html[start..].find('>')? + start + 1;
    let end = start + html[start..].to_lowercase().find("</title>")?;
    Some(html[start..end].trim().to_string())
}

fn extract_meta_property(html: &str, property: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let prop_lower = property.to_lowercase();
    let mut pos = 0;
    while let Some(tag_start) = lower[pos..].find("<meta") {
        let tag_start = pos + tag_start;
        let tag_end = html[tag_start..].find('>').map(|e| tag_start + e + 1).unwrap_or(html.len());
        let tag = &html[tag_start..tag_end];
        let tag_lower = &lower[tag_start..tag_end];
        if tag_lower.contains(&prop_lower) {
            if let Some(content) = extract_attr(tag, "content") {
                return Some(content);
            }
        }
        pos = tag_end;
    }
    None
}

fn itunes_duration_to_upnp(d: String) -> String {
    let parts: Vec<&str> = d.trim().split(':').collect();
    match parts.len() {
        1 => {
            if let Ok(secs) = parts[0].parse::<u64>() {
                let h = secs / 3600;
                let m = (secs % 3600) / 60;
                let s = secs % 60;
                return format!("{}:{:02}:{:02}.000", h, m, s);
            }
        }
        2 => return format!("0:{}.000", d),
        3 => return format!("{}.000", d),
        _ => {}
    }
    d
}
