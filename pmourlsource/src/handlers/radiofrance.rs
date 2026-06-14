use crate::handler::{ResolvedContent, ResolvedTrack, UrlHandler, UrlResolverError};
use async_trait::async_trait;
use reqwest::{redirect, Client};

/// Handler dédié aux URLs radiofrance.fr — priorité 80.
///
/// RadioFrance utilise SvelteKit (SSR). La clé `rssFeed:` est inline dans le JS
/// de la page pour les pages podcast standard, mais vide pour les pages série.
///
/// Stratégie selon le type d'URL :
///
/// 1. **Page podcast** (`/podcasts/{slug}`, sans sous-chemin épisode)
///    → chercher `rssFeed:"https://..."` → fetch + parse RSS
///
/// 2. **Page série** (`/podcasts/serie-{slug}`)
///    → parser le JSON-LD `ItemList` → extraire le slug du podcast sous-jacent
///    → fetch page podcast → trouver rssFeed
///
/// 3. **Page épisode** (`/podcasts/{podcast-slug}/{episode-slug}-{id}`)
///    → extraire l'URL MP3 directe
pub struct RadioFranceUrlHandler {
    client: Client,
}

impl RadioFranceUrlHandler {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .redirect(redirect::Policy::limited(5))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(20))
            .build()?;
        Ok(Self { client })
    }

    async fn resolve_inner(&self, url: &str) -> Result<ResolvedContent, UrlResolverError> {
        let html = self.fetch_html(url).await?;

        // --- Cas 1 : page podcast standard → rssFeed non vide ---
        if let Some(rss_url) = extract_rss_feed_key(&html) {
            tracing::debug!(rss_url = %rss_url, "RadioFrance: rssFeed trouvé directement");
            return self.fetch_rss(&rss_url).await;
        }

        // --- Cas 2 : page série → JSON-LD ItemList → podcast sous-jacent ---
        if let Some(podcast_url) = derive_podcast_url_from_series(&html, url) {
            tracing::debug!(podcast_url = %podcast_url, "RadioFrance: série → page podcast");
            let podcast_html = self.fetch_html(&podcast_url).await?;
            if let Some(rss_url) = extract_rss_feed_key(&podcast_html) {
                tracing::debug!(rss_url = %rss_url, "RadioFrance: rssFeed via série");
                return self.fetch_rss(&rss_url).await;
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
        self.client
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

/// Pour une page série, extrait les URLs d'épisodes du JSON-LD `ItemList`,
/// déduit le slug du podcast sous-jacent et construit l'URL de sa page.
///
/// Exemple :
///   épisode : `https://www.radiofrance.fr/franceculture/podcasts/les-contes-des-mille-et-une-sciences/kasparov-7422833`
///   → page podcast : `https://www.radiofrance.fr/franceculture/podcasts/les-contes-des-mille-et-une-sciences`
fn derive_podcast_url_from_series(html: &str, series_url: &str) -> Option<String> {
    // Extraire la première URL d'épisode depuis le JSON-LD ItemList
    let item_marker = "\"@type\":\"ItemList\"";
    let list_pos = html.find(item_marker)?;
    let after_list = &html[list_pos..];

    // Chercher "url":"https://www.radiofrance.fr/..."
    let url_needle = "\"url\":\"https://www.radiofrance.fr/";
    let pos = after_list.find(url_needle)? + url_needle.len() - "https://www.radiofrance.fr/".len();
    let from = list_pos + pos + "\"url\":\"".len();
    let end = html[from..].find('"')? + from;
    let episode_url = &html[from..end];

    // L'URL épisode : https://www.radiofrance.fr/{station}/podcasts/{podcast-slug}/{episode-slug}-{id}
    // On veut : https://www.radiofrance.fr/{station}/podcasts/{podcast-slug}
    // Compter les segments du path (après le domaine)
    let _after_domain = episode_url.find("/")?..; // find the first /
    let path = &episode_url[episode_url.find("radiofrance.fr/")? + "radiofrance.fr".len()..];
    // path = /{station}/podcasts/{podcast-slug}/{episode-slug}
    let segments: Vec<&str> = path.trim_start_matches('/').splitn(5, '/').collect();
    // segments = [station, "podcasts", podcast-slug, episode-slug]
    if segments.len() < 3 {
        return None;
    }
    // Éviter de boucler sur la même URL série
    let podcast_slug = segments[2];
    if podcast_slug.starts_with("serie-") {
        return None;
    }
    let podcast_url = format!(
        "https://www.radiofrance.fr/{}/{}/{}",
        segments[0], segments[1], podcast_slug
    );
    // Ne pas retourner l'URL série elle-même
    if podcast_url == series_url.trim_end_matches('/') {
        return None;
    }
    Some(podcast_url)
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
