use crate::handler::{ResolvedContent, ResolvedTrack, UrlHandler, UrlResolverError};
use async_trait::async_trait;
use reqwest::{redirect, Client};

/// Handler générique de dernier recours — priorité 10.
///
/// Pipeline :
///   1. Garde-fou SSRF (rejette les IPs privées/locales)
///   2. GET avec suivi de redirections (max 5)
///   3. Détection par Content-Type :
///      - audio/*                    → Stream direct
///      - application/rss+xml, …     → parse RSS/Atom → Playlist
///      - .m3u / .pls / .xspf        → parse playlist → Playlist
///   4. text/html → cherche :
///      - <link type="application/rss+xml"> → fetch RSS → Playlist
///      - <audio src="…">
///      - og:audio / og:url audio
pub struct GenericUrlHandler {
    client: Client,
}

impl GenericUrlHandler {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .redirect(redirect::Policy::limited(5))
            .user_agent("PMOMusic/1.0")
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        Ok(Self { client })
    }

    /// Rejette les URLs ciblant des réseaux privés/locaux (SSRF).
    fn is_safe_url(url: &str) -> bool {
        let Ok(parsed) = url::Url::parse(url) else {
            return false;
        };
        let Some(host) = parsed.host_str() else {
            return false;
        };
        // Rejeter loopback, link-local, et RFC-1918
        if host == "localhost" || host == "127.0.0.1" || host == "::1" {
            return false;
        }
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            return !ip.is_loopback() && !ip.is_unspecified() && is_public_ip(ip);
        }
        true
    }

    async fn fetch_and_resolve(&self, url: &str) -> Result<ResolvedContent, UrlResolverError> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| UrlResolverError::ResolutionFailed(e.to_string()))?;

        let final_url = resp.url().to_string();
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let body = resp
            .text()
            .await
            .map_err(|e| UrlResolverError::ResolutionFailed(e.to_string()))?;

        // Audio direct
        if content_type.starts_with("audio/") {
            let mime = content_type.split(';').next().unwrap_or("audio/mpeg").trim().to_string();
            let title = title_from_url(&final_url);
            return Ok(ResolvedContent::Stream {
                uri: final_url,
                title,
                mime_type: mime,
            });
        }

        // Playlist M3U
        if content_type.contains("mpegurl") || final_url.ends_with(".m3u") || final_url.ends_with(".m3u8") {
            return parse_m3u(&body, &final_url);
        }

        // Playlist PLS
        if content_type.contains("scpls") || final_url.ends_with(".pls") {
            return parse_pls(&body, &final_url);
        }

        // RSS / Atom / podcast
        if is_rss_content_type(&content_type) || final_url.ends_with(".xml") {
            return parse_rss(&body, &final_url);
        }

        // HTML — chercher RSS link puis audio elements
        if content_type.starts_with("text/html") || content_type.is_empty() {
            return self.scrape_html(&body, &final_url).await;
        }

        Err(UrlResolverError::NotSupported(format!(
            "Content-Type non géré : {}",
            content_type
        )))
    }

    async fn scrape_html(&self, html: &str, base_url: &str) -> Result<ResolvedContent, UrlResolverError> {
        // 1. Chercher un lien RSS (<link type="application/rss+xml" href="...">)
        if let Some(rss_url) = extract_rss_link(html, base_url) {
            tracing::debug!(rss_url = %rss_url, "HTML scraper found RSS feed");
            if Self::is_safe_url(&rss_url) {
                if let Ok(resp) = self.client.get(&rss_url).send().await {
                    if let Ok(body) = resp.text().await {
                        if let Ok(result) = parse_rss(&body, &rss_url) {
                            return Ok(result);
                        }
                    }
                }
            }
        }

        // 2. Chercher <audio src="...">
        if let Some(audio_url) = extract_audio_src(html, base_url) {
            tracing::debug!(audio_url = %audio_url, "HTML scraper found <audio>");
            let title = extract_og_title(html)
                .or_else(|| extract_title_tag(html))
                .unwrap_or_else(|| title_from_url(base_url));
            return Ok(ResolvedContent::Track(ResolvedTrack {
                uri: audio_url,
                title,
                artist: None,
                album: None,
                duration: None,
                album_art: extract_og_image(html),
                mime_type: "audio/mpeg".to_string(),
            }));
        }

        // 3. og:audio
        if let Some(audio_url) = extract_og_audio(html) {
            tracing::debug!(audio_url = %audio_url, "HTML scraper found og:audio");
            let title = extract_og_title(html)
                .or_else(|| extract_title_tag(html))
                .unwrap_or_else(|| title_from_url(base_url));
            return Ok(ResolvedContent::Track(ResolvedTrack {
                uri: audio_url,
                title,
                artist: None,
                album: None,
                duration: None,
                album_art: extract_og_image(html),
                mime_type: "audio/mpeg".to_string(),
            }));
        }

        Err(UrlResolverError::NotSupported(format!(
            "Aucun contenu audio trouvé dans la page : {}",
            base_url
        )))
    }
}

impl Default for GenericUrlHandler {
    fn default() -> Self {
        Self::new().expect("Failed to build HTTP client")
    }
}

#[async_trait]
impl UrlHandler for GenericUrlHandler {
    fn name(&self) -> &str {
        "GenericUrlHandler"
    }

    fn priority(&self) -> u8 {
        10
    }

    fn can_handle(&self, url: &str) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
    }

    async fn resolve(&self, url: &str) -> Result<ResolvedContent, UrlResolverError> {
        if !Self::is_safe_url(url) {
            return Err(UrlResolverError::SsrfBlocked);
        }
        self.fetch_and_resolve(url).await
    }
}

// ── Parseurs ────────────────────────────────────────────────────────────────

fn parse_rss(body: &str, source_url: &str) -> Result<ResolvedContent, UrlResolverError> {
    let mut items: Vec<ResolvedTrack> = Vec::new();
    let mut feed_title: Option<String> = None;
    let mut feed_image: Option<String> = None;
    let mut current_title: Option<String> = None;
    let mut current_uri: Option<String> = None;
    let mut current_duration: Option<String> = None;
    let mut current_date: Option<String> = None;
    let mut current_image: Option<String> = None;
    let mut in_item = false;

    // Parsing XML ligne par ligne — quick_xml non disponible ici,
    // on utilise une approche par extraction de patterns XML simples.
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
            current_date = None;
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
                // Vérifier que c'est bien de l'audio
                let type_ = extract_attr(trimmed, "type").unwrap_or_default();
                if type_.starts_with("audio/") || type_.is_empty() {
                    current_uri = Some(url);
                }
            }
        } else if trimmed.starts_with("<itunes:duration") {
            current_duration = extract_xml_text(trimmed, "itunes:duration");
        } else if trimmed.starts_with("<pubDate") {
            current_date = extract_xml_text(trimmed, "pubDate");
        } else if trimmed.starts_with("<itunes:image") {
            if let Some(href) = extract_attr(trimmed, "href") {
                current_image = Some(href);
            }
        }
    }

    if items.is_empty() {
        return Err(UrlResolverError::NotSupported(format!(
            "Aucun épisode audio dans le feed RSS : {}",
            source_url
        )));
    }

    Ok(ResolvedContent::Playlist {
        title: feed_title,
        items,
    })
}

fn parse_m3u(body: &str, _source_url: &str) -> Result<ResolvedContent, UrlResolverError> {
    let mut items: Vec<ResolvedTrack> = Vec::new();
    let mut pending_title: Option<String> = None;

    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line == "#EXTM3U" {
            continue;
        }
        if let Some(info) = line.strip_prefix("#EXTINF:") {
            // #EXTINF:<duration>,<title>
            let title = info.splitn(2, ',').nth(1).unwrap_or("").trim().to_string();
            if !title.is_empty() {
                pending_title = Some(title);
            }
        } else if !line.starts_with('#') {
            let title = pending_title.take().unwrap_or_else(|| title_from_url(line));
            items.push(ResolvedTrack::new(line, title));
        }
    }

    if items.is_empty() {
        return Err(UrlResolverError::NotSupported("M3U vide".to_string()));
    }

    if items.len() == 1 {
        return Ok(ResolvedContent::Stream {
            uri: items.remove(0).uri,
            title: items.first().map(|t| t.title.clone()).unwrap_or_default(),
            mime_type: "audio/mpeg".to_string(),
        });
    }

    Ok(ResolvedContent::Playlist { title: None, items })
}

fn parse_pls(body: &str, _source_url: &str) -> Result<ResolvedContent, UrlResolverError> {
    let mut uris: Vec<String> = Vec::new();
    let mut titles: Vec<String> = Vec::new();

    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.to_lowercase().strip_prefix("file") {
            if let Some(url) = rest.splitn(2, '=').nth(1) {
                uris.push(url.trim().to_string());
            }
        } else if let Some(rest) = line.to_lowercase().strip_prefix("title") {
            if let Some(t) = rest.splitn(2, '=').nth(1) {
                titles.push(t.trim().to_string());
            }
        }
    }

    if uris.is_empty() {
        return Err(UrlResolverError::NotSupported("PLS vide".to_string()));
    }

    let items: Vec<ResolvedTrack> = uris
        .into_iter()
        .enumerate()
        .map(|(i, uri)| {
            let title = titles.get(i).cloned().unwrap_or_else(|| title_from_url(&uri));
            ResolvedTrack::new(uri, title)
        })
        .collect();

    if items.len() == 1 {
        let item = items.into_iter().next().unwrap();
        return Ok(ResolvedContent::Stream {
            uri: item.uri,
            title: item.title,
            mime_type: "audio/mpeg".to_string(),
        });
    }

    Ok(ResolvedContent::Playlist { title: None, items })
}

// ── Utilitaires d'extraction HTML/XML ───────────────────────────────────────

fn extract_rss_link(html: &str, base_url: &str) -> Option<String> {
    // <link ... type="application/rss+xml" ... href="URL" ...>
    // ou  <link ... href="URL" ... type="application/rss+xml" ...>
    let lower = html.to_lowercase();
    let mut pos = 0;
    while let Some(start) = lower[pos..].find("<link") {
        let start = pos + start;
        let end = html[start..].find('>').map(|e| start + e + 1).unwrap_or(html.len());
        let tag = &html[start..end];
        let tag_lower = &lower[start..end];

        if tag_lower.contains("application/rss+xml") || tag_lower.contains("application/atom+xml") {
            if let Some(href) = extract_attr(tag, "href") {
                return Some(resolve_url(base_url, &href));
            }
        }
        pos = end;
    }
    None
}

fn extract_audio_src(html: &str, base_url: &str) -> Option<String> {
    let lower = html.to_lowercase();
    if let Some(start) = lower.find("<audio") {
        let end = html[start..].find('>').map(|e| start + e + 1).unwrap_or(html.len());
        let tag = &html[start..end];
        if let Some(src) = extract_attr(tag, "src") {
            return Some(resolve_url(base_url, &src));
        }
        // <source src="..."> inside <audio>
        let after = &html[end..];
        let lower_after = after.to_lowercase();
        if let Some(src_start) = lower_after.find("<source") {
            let src_end = after[src_start..].find('>').map(|e| src_start + e + 1).unwrap_or(after.len());
            let src_tag = &after[src_start..src_end];
            if let Some(src) = extract_attr(src_tag, "src") {
                return Some(resolve_url(base_url, &src));
            }
        }
    }
    None
}

fn extract_og_audio(html: &str) -> Option<String> {
    extract_meta_property(html, "og:audio")
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

/// Extrait la valeur d'un attribut HTML/XML depuis une balise.
/// Gère les guillemets simples, doubles et sans guillemets.
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

/// Extrait le contenu texte d'un élément XML simple sur une seule ligne.
fn extract_xml_text(line: &str, tag: &str) -> Option<String> {
    // Chercher <tag> ou <tag ...>
    let open_plain = format!("<{}>", tag);
    let open_with_attrs = format!("<{} ", tag);
    let close = format!("</{}>", tag);

    let content_start = if let Some(p) = line.find(&open_plain) {
        p + open_plain.len()
    } else if let Some(p) = line.find(&open_with_attrs) {
        // Avancer jusqu'à la fermeture de la balise ouvrante
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

/// Résout une URL relative par rapport à une base.
fn resolve_url(base: &str, target: &str) -> String {
    if target.starts_with("http://") || target.starts_with("https://") {
        return target.to_string();
    }
    if target.starts_with("//") {
        let scheme = if base.starts_with("https") { "https" } else { "http" };
        return format!("{}:{}", scheme, target);
    }
    if let Ok(base_url) = url::Url::parse(base) {
        if let Ok(resolved) = base_url.join(target) {
            return resolved.to_string();
        }
    }
    target.to_string()
}

/// Extrait un titre lisible depuis une URL.
fn title_from_url(url: &str) -> String {
    url.rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or(url)
        .split('?')
        .next()
        .unwrap_or(url)
        .replace(['-', '_'], " ")
        .to_string()
}

/// Détermine si le Content-Type est RSS/Atom.
fn is_rss_content_type(ct: &str) -> bool {
    ct.contains("rss") || ct.contains("atom") || ct.contains("xml")
}

/// Convertit une durée iTunes ("HH:MM:SS" ou "MM:SS" ou secondes) en format UPnP ("H:MM:SS.000").
fn itunes_duration_to_upnp(d: String) -> String {
    let parts: Vec<&str> = d.trim().split(':').collect();
    match parts.len() {
        1 => {
            // Secondes brutes
            if let Ok(secs) = parts[0].parse::<u64>() {
                let h = secs / 3600;
                let m = (secs % 3600) / 60;
                let s = secs % 60;
                return format!("{}:{:02}:{:02}.000", h, m, s);
            }
        }
        2 => {
            return format!("0:{}.000", d);
        }
        3 => {
            return format!("{}.000", d);
        }
        _ => {}
    }
    d
}

/// Vérifie qu'une IP est publique (non privée, non loopback, non link-local).
fn is_public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            !v4.is_private()
                && !v4.is_loopback()
                && !v4.is_link_local()
                && !v4.is_broadcast()
                && !v4.is_documentation()
                && !v4.is_unspecified()
        }
        std::net::IpAddr::V6(v6) => {
            !v6.is_loopback() && !v6.is_unspecified() && !is_v6_link_local(v6)
        }
    }
}

fn is_v6_link_local(ip: std::net::Ipv6Addr) -> bool {
    // fe80::/10
    ip.segments()[0] & 0xffc0 == 0xfe80
}
