use crate::handler::{ResolvedContent, UrlHandler, UrlResolverError};
use async_trait::async_trait;

/// Résout les URLs de partage Qobuz vers des container_ids natifs.
///
/// Supporte open.qobuz.com et play.qobuz.com.
/// Les IDs peuvent être alphanumériques pour tous les types (album, track, playlist, artist).
///
/// Exemples :
///   https://open.qobuz.com/album/l46fxnqnxp5vs  → qobuz:album:l46fxnqnxp5vs
///   https://open.qobuz.com/track/48471123        → qobuz:track:48471123
///   https://open.qobuz.com/playlist/63246908     → qobuz:playlist:63246908
///   https://open.qobuz.com/artist/125709         → qobuz:artist:125709
pub struct QobuzUrlHandler;

impl QobuzUrlHandler {
    pub fn new() -> Self {
        Self
    }

    fn parse(&self, url: &str) -> Option<(String, String)> {
        // Localiser "qobuz.com/" dans l'URL
        let after_domain = url.find("qobuz.com/").map(|i| &url[i + "qobuz.com".len()..])?;

        // after_domain commence par "/"
        let path = after_domain.trim_start_matches('/');
        let mut parts = path.splitn(3, '/');

        let type_ = parts.next().unwrap_or("");
        let id_raw = parts.next().unwrap_or("");
        // Supprimer les query params éventuels (#, ?)
        let id = id_raw.split('?').next().unwrap_or(id_raw);
        let id = id.split('#').next().unwrap_or(id);

        match type_ {
            "album" | "track" | "playlist" | "artist" => {
                if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()) {
                    Some((type_.to_string(), id.to_string()))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl Default for QobuzUrlHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UrlHandler for QobuzUrlHandler {
    fn name(&self) -> &str {
        "QobuzUrlHandler"
    }

    fn priority(&self) -> u8 {
        90
    }

    fn can_handle(&self, url: &str) -> bool {
        url.contains("qobuz.com/")
    }

    async fn resolve(&self, url: &str) -> Result<ResolvedContent, UrlResolverError> {
        let (type_, id) = self
            .parse(url)
            .ok_or_else(|| UrlResolverError::NotSupported(url.to_string()))?;

        let container_id = format!("qobuz:{}:{}", type_, id);

        tracing::debug!(
            url = %url,
            container_id = %container_id,
            "QobuzUrlHandler resolved"
        );

        Ok(ResolvedContent::SourceContainer {
            source_id: "qobuz".to_string(),
            container_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_album_alphanumeric_id() {
        let h = QobuzUrlHandler::new();
        let r = h
            .resolve("https://open.qobuz.com/album/l46fxnqnxp5vs")
            .await
            .unwrap();
        let ResolvedContent::SourceContainer { container_id, .. } = r else { panic!("unexpected variant") };
        assert_eq!(container_id, "qobuz:album:l46fxnqnxp5vs");
    }

    #[tokio::test]
    async fn test_track_numeric_id() {
        let h = QobuzUrlHandler::new();
        let r = h
            .resolve("https://open.qobuz.com/track/48471123")
            .await
            .unwrap();
        let ResolvedContent::SourceContainer { container_id, .. } = r else { panic!("unexpected variant") };
        assert_eq!(container_id, "qobuz:track:48471123");
    }

    #[tokio::test]
    async fn test_play_subdomain() {
        let h = QobuzUrlHandler::new();
        let r = h
            .resolve("https://play.qobuz.com/album/l46fxnqnxp5vs")
            .await
            .unwrap();
        let ResolvedContent::SourceContainer { container_id, .. } = r else { panic!("unexpected variant") };
        assert_eq!(container_id, "qobuz:album:l46fxnqnxp5vs");
    }

    #[tokio::test]
    async fn test_unknown_type_rejected() {
        let h = QobuzUrlHandler::new();
        let r = h.resolve("https://open.qobuz.com/label/123").await;
        assert!(r.is_err());
    }

    #[test]
    fn test_can_handle() {
        let h = QobuzUrlHandler::new();
        assert!(h.can_handle("https://open.qobuz.com/album/abc"));
        assert!(!h.can_handle("https://www.spotify.com/album/abc"));
    }
}
