use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UrlResolverError {
    #[error("URL non reconnue : {0}")]
    NotSupported(String),
    #[error("Résolution échouée : {0}")]
    ResolutionFailed(String),
    #[error("URL bloquée (réseau privé/local)")]
    SsrfBlocked,
}

/// Un track résolu depuis une source externe (RSS enclosure, audio direct…)
#[derive(Debug, Clone)]
pub struct ResolvedTrack {
    /// URL directe de l'audio (jouable par le renderer)
    pub uri: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<String>,   // format "H:MM:SS.mmm" UPnP
    pub album_art: Option<String>,
    pub mime_type: String,          // ex. "audio/mpeg", "audio/aac"
}

impl ResolvedTrack {
    pub fn new(uri: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            title: title.into(),
            artist: None,
            album: None,
            duration: None,
            album_art: None,
            mime_type: "audio/mpeg".to_string(),
        }
    }
}

/// Contenu résolu depuis une URL externe
#[derive(Debug)]
pub enum ResolvedContent {
    /// Référence à un container d'une source existante.
    /// La UrlSource retourne un stub container avec cet ID ; le content directory
    /// le route naturellement vers la source propriétaire lors du browse.
    SourceContainer {
        source_id: String,
        container_id: String,
    },

    /// Liste ordonnée de tracks (RSS/podcast, M3U, PLS, XSPF…)
    Playlist {
        title: Option<String>,
        items: Vec<ResolvedTrack>,
    },

    /// Flux continu (radio, stream live)
    Stream {
        uri: String,
        title: String,
        mime_type: String,
    },

    /// Track unique identifié directement
    Track(ResolvedTrack),
}

/// Trait implémenté par chaque handler spécialisé (Qobuz, RadioFrance…)
/// et par le handler générique de dernier recours.
#[async_trait]
pub trait UrlHandler: Send + Sync {
    fn name(&self) -> &str;
    /// Priorité : plus grand = essayé en premier. Défaut : 50.
    fn priority(&self) -> u8 {
        50
    }
    /// Filtre rapide sans I/O — simple test regex/contains sur l'URL.
    fn can_handle(&self, url: &str) -> bool;
    /// Résolution effective (I/O autorisé).
    async fn resolve(&self, url: &str) -> Result<ResolvedContent, UrlResolverError>;
}

/// Registre ordonné de handlers. Les handlers sont triés par priorité décroissante.
pub struct UrlResolver {
    handlers: Vec<Box<dyn UrlHandler>>,
}

impl std::fmt::Debug for UrlResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UrlResolver")
            .field("handlers", &format!("{} handlers", self.handlers.len()))
            .finish()
    }
}

impl UrlResolver {
    pub fn new() -> Self {
        Self { handlers: vec![] }
    }

    pub fn register(&mut self, handler: Box<dyn UrlHandler>) {
        self.handlers.push(handler);
        self.handlers
            .sort_by(|a, b| b.priority().cmp(&a.priority()));
    }

    pub async fn resolve(&self, url: &str) -> Result<ResolvedContent, UrlResolverError> {
        for handler in &self.handlers {
            if handler.can_handle(url) {
                return handler.resolve(url).await;
            }
        }
        Err(UrlResolverError::NotSupported(url.to_string()))
    }
}

impl Default for UrlResolver {
    fn default() -> Self {
        Self::new()
    }
}
