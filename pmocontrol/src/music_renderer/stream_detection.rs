//! Stream detection utilities for identifying continuous streams (radio) vs bounded media.
//!
//! This module provides utilities to detect whether a given URL points to a continuous
//! stream (like a radio station) or a bounded media file by analyzing HTTP headers.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use tracing::{debug, trace};
use ureq::Agent;

/// Cache global des résultats de détection de stream par URL
static STREAM_CACHE: LazyLock<Arc<Mutex<HashMap<String, bool>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Set des URLs actuellement en cours de vérification (pour éviter les doublons)
static PENDING_CHECKS: LazyLock<Arc<Mutex<HashSet<String>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HashSet::new())));

/// Default timeout for HTTP HEAD requests when detecting stream type
const DEFAULT_STREAM_DETECTION_TIMEOUT_SECS: u64 = 3;

/// Détecte si une URL correspond à un flux continu (radio sans durée définie).
///
/// Cette fonction effectue une requête HTTP HEAD sur l'URL fournie et analyse les headers
/// de la réponse pour déterminer si c'est un flux continu ou un fichier avec durée définie.
///
/// **Optimisation** : Les résultats sont mis en cache pour éviter de refaire la détection
/// sur la même URL. Si une détection est déjà en cours pour cette URL, la fonction attend
/// ou retourne false pour ne pas bloquer.
///
/// # Critères de détection d'un flux continu :
///
/// - Absence de header `Content-Length` (pas de taille définie)
/// - OU présence de `Transfer-Encoding: chunked` sans `Content-Length`
/// - OU `Content-Type` indiquant un stream (audio/mpeg avec icy-*, application/ogg, etc.)
/// - OU présence de headers ICY (Icecast/Shoutcast) qui indiquent toujours un stream
///
/// # Arguments
///
/// * `url` - L'URL à analyser
///
/// # Returns
///
/// `true` si l'URL correspond à un flux continu, `false` sinon.
/// En cas d'erreur de connexion, retourne `false` par défaut (considéré comme non-stream).
pub fn is_continuous_stream_url(url: &str) -> bool {
    // Quick checks on URL pattern before making HTTP request
    if is_known_stream_pattern(url) {
        debug!("URL {} matches known stream pattern", url);
        return true;
    }

    // Check cache first
    {
        let cache = STREAM_CACHE.lock().unwrap();
        if let Some(&cached_result) = cache.get(url) {
            trace!("Cache hit for {}: is_stream={}", url, cached_result);
            return cached_result;
        }
    }

    // Check if already being verified
    {
        let mut pending = PENDING_CHECKS.lock().unwrap();
        if pending.contains(url) {
            debug!(
                "Stream detection already in progress for {}, returning false temporarily",
                url
            );
            return false;
        }
        // Mark as pending
        pending.insert(url.to_string());
    }

    // Spawn background task for detection
    let url_owned = url.to_string();
    std::thread::spawn(move || {
        let result = match check_stream_headers(&url_owned) {
            Ok(is_stream) => {
                trace!("Stream detection for {}: {}", url_owned, is_stream);
                is_stream
            }
            Err(e) => {
                debug!(
                    "Failed to detect stream type for {}: {}, assuming non-stream",
                    url_owned, e
                );
                false
            }
        };

        // Store in cache
        {
            let mut cache = STREAM_CACHE.lock().unwrap();
            cache.insert(url_owned.clone(), result);
        }

        // Remove from pending
        {
            let mut pending = PENDING_CHECKS.lock().unwrap();
            pending.remove(&url_owned);
        }

        debug!(
            "Stream detection completed for {}: is_stream={}",
            url_owned, result
        );
    });

    // Return false temporarily while detection is in progress
    // The watcher will pick up the correct value on next poll
    false
}

/// Vérifie si l'URL correspond à un pattern connu de streaming
fn is_known_stream_pattern(url: &str) -> bool {
    let url_lower = url.to_lowercase();

    // Common streaming endpoints
    url_lower.contains("/stream")
        || url_lower.contains("/live")
        || url_lower.contains("/radio")
        || url_lower.contains(".pls")
        || url_lower.contains(".m3u")
        || url_lower.ends_with(":8000")
        || url_lower.ends_with(":8080")
}

/// Effectue une requête HTTP HEAD et analyse les headers
fn check_stream_headers(url: &str) -> Result<bool, String> {
    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(
            DEFAULT_STREAM_DETECTION_TIMEOUT_SECS,
        )))
        .build()
        .into();

    let response = agent
        .head(url)
        .call()
        .map_err(|e| format!("HTTP HEAD request failed: {}", e))?;

    // Check for ICY headers (Icecast/Shoutcast) - always indicates streaming
    if response.headers().get("icy-name").is_some()
        || response.headers().get("icy-metaint").is_some()
        || response.headers().get("ice-audio-info").is_some()
    {
        debug!("ICY headers detected for {}, this is a stream", url);
        return Ok(true);
    }

    // Check Content-Length
    let has_content_length = response.headers().get("content-length").is_some();

    // Check Transfer-Encoding
    let is_chunked = response
        .headers()
        .get("transfer-encoding")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_lowercase().contains("chunked"))
        .unwrap_or(false);

    // Check Content-Type for streaming indicators
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let content_type_lower = content_type.to_lowercase();

    let is_streaming_mime = content_type_lower.contains("audio/mpeg")
        || content_type_lower.contains("audio/aac")
        || content_type_lower.contains("audio/aacp")
        || content_type_lower.contains("application/ogg")
        || content_type_lower.contains("audio/ogg");

    // Decision logic:
    // - No Content-Length + streaming MIME = stream
    // - Chunked encoding without Content-Length = likely stream
    // - Has Content-Length = bounded media (not a stream)

    let is_stream = !has_content_length && (is_streaming_mime || is_chunked);

    trace!(
        "Stream detection for {}: content-length={}, chunked={}, streaming_mime={}, is_stream={}",
        url,
        has_content_length,
        is_chunked,
        is_streaming_mime,
        is_stream
    );

    Ok(is_stream)
}

/// Canonical check: returns `true` if this item should be treated as a continuous stream.
///
/// Checks `metadata.is_continuous_stream` first (already computed at ingest time),
/// then falls back to the URL-based HTTP detection.
///
/// Use this function everywhere transport-layer code needs to decide whether playback is
/// a continuous stream (radio) vs bounded media (file/album track).
pub fn is_continuous_stream(metadata: Option<&crate::model::TrackMetadata>, uri: &str) -> bool {
    metadata.map(|m| m.is_continuous_stream).unwrap_or(false) || is_continuous_stream_url(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_stream_patterns() {
        assert!(is_known_stream_pattern("http://example.com/stream"));
        assert!(is_known_stream_pattern("http://example.com/live"));
        assert!(is_known_stream_pattern("http://example.com/radio.mp3"));
        assert!(is_known_stream_pattern("http://example.com:8000/"));
        assert!(is_known_stream_pattern("http://example.com/playlist.m3u"));

        assert!(!is_known_stream_pattern("http://example.com/music.mp3"));
        assert!(!is_known_stream_pattern("http://example.com/file.flac"));
    }
}
