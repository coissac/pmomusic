//! UriSource - Source audio depuis une URI arbitraire
//!
//! Ouvre une URI (fichier local ou HTTP/HTTPS), décode l'audio (tout format
//! supporté par `pmoflac` : FLAC, MP3, OGG, WAV, AIFF) et émet des `AudioSegment`
//! vers un sender tokio.
//!
//! # Usage
//!
//! ```rust,no_run
//! use pmoaudio_ext::sources::UriSource;
//! use tokio_util::sync::CancellationToken;
//! use tokio::sync::mpsc;
//! use std::sync::Arc;
//! use pmoaudio::AudioSegment;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let (tx, mut rx) = mpsc::channel::<Arc<AudioSegment>>(64);
//! let stop = CancellationToken::new();
//!
//! let source = UriSource::open("/music/track.flac", 0.0, stop.clone()).await?;
//! println!("Duration: {:?}", source.duration_sec());
//!
//! let eof = source.emit_to_channel(&tx, &stop).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Seek
//!
//! Implémenté par skip des frames initiales. Pour les formats sans seek natif
//! (MP3, stream HTTP), tout le contenu est lu mais les frames avant `seek_sec`
//! ne sont pas émises.

use std::sync::Arc;

use pmoaudio::{AudioSegment, nodes::AudioError};
use pmoflac::{StreamInfo, decode_audio_stream};
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use super::pcm_decode::{bytes_to_segment, validate_stream};

const CHUNK_FRAMES: usize = 2048; // ~46ms @ 44.1kHz

/// Source audio ouverte depuis une URI, prête à émettre des segments.
pub struct UriSource {
    reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    stream_info: StreamInfo,
    frames_to_skip: u64,
    /// true si c'est un flux continu (radio, stream) sans durée définie
    pub is_continuous: bool,
}

impl UriSource {
    /// Ouvre une URI et prépare la source.
    ///
    /// - Chemin absolu ou `file://...` → fichier local
    /// - `http://...` / `https://...` → streaming HTTP
    pub async fn open(
        uri: &str,
        seek_sec: f64,
        stop_token: CancellationToken,
    ) -> Result<Self, AudioError> {
        if uri.starts_with("http://") || uri.starts_with("https://") {
            Self::open_http(uri, seek_sec, &stop_token).await
        } else {
            let path = uri.strip_prefix("file://").unwrap_or(uri);
            Self::open_file(path, seek_sec).await
        }
    }

    /// Durée totale en secondes, si connue.
    pub fn duration_sec(&self) -> Option<f64> {
        let info = &self.stream_info;
        info.total_samples
            .filter(|&s| s > 0)
            .map(|s| s as f64 / info.sample_rate as f64)
    }

    /// Nombre total de samples à 96 kHz après resampling, si connu.
    /// Utilisé pour renseigner STREAMINFO.total_samples dans le FLAC de sortie.
    pub fn total_samples_at(&self, output_sample_rate: u32) -> Option<u64> {
        let info = &self.stream_info;
        info.total_samples.filter(|&s| s > 0).map(|s| {
            // Convertir le nombre de samples source vers le sample rate de sortie
            let ratio = output_sample_rate as f64 / info.sample_rate as f64;
            (s as f64 * ratio).round() as u64
        })
    }

    /// Retourne true si c'est un flux continu (radio, stream) sans durée définie.
    pub fn is_continuous(&self) -> bool {
        self.is_continuous
    }

    /// Émet les chunks audio vers `tx`.
    ///
    /// Retourne `Ok(true)` si EOF naturel, `Ok(false)` si annulé ou receiver fermé.
    pub async fn emit_to_channel(
        mut self,
        tx: &mpsc::Sender<Arc<AudioSegment>>,
        stop_token: &CancellationToken,
    ) -> Result<bool, AudioError> {
        let info = self.stream_info.clone();
        let bytes_per_sample = info.bytes_per_sample();
        let frame_bytes = bytes_per_sample * info.channels as usize;
        let chunk_byte_len = CHUNK_FRAMES * frame_bytes;

        let mut pending = Vec::new();
        let mut read_buf = vec![0u8; frame_bytes * 512.max(CHUNK_FRAMES)];
        let mut chunk_index = 0u64;
        let mut total_frames = 0u64;

        loop {
            tokio::select! {
                _ = stop_token.cancelled() => {
                    debug!("UriSource: cancelled");
                    return Ok(false);
                }

                read_result = self.reader.read(&mut read_buf) => {
                    let read = read_result
                        .map_err(|e| AudioError::IoError(e.to_string()))?;

                    if read == 0 {
                        break; // EOF
                    }

                    pending.extend_from_slice(&read_buf[..read]);

                    while pending.len() >= chunk_byte_len {
                        let chunk_bytes = pending.drain(..chunk_byte_len).collect::<Vec<_>>();
                        let frames = CHUNK_FRAMES;

                        // Seek : ignorer les frames avant la position demandée
                        if total_frames + frames as u64 <= self.frames_to_skip {
                            total_frames += frames as u64;
                            chunk_index += 1;
                            continue;
                        }

                        let timestamp_sec = total_frames as f64 / info.sample_rate as f64;
                        let segment = bytes_to_segment(&chunk_bytes, &info, frames, chunk_index, timestamp_sec)?;

                        if tx.send(segment).await.is_err() {
                            debug!("UriSource: receiver dropped");
                            return Ok(false);
                        }

                        total_frames += frames as u64;
                        chunk_index += 1;
                    }
                }
            }
        }

        // Émettre le reste (< un chunk complet)
        if !pending.is_empty() {
            let frames = pending.len() / frame_bytes;
            if frames > 0 && total_frames >= self.frames_to_skip {
                let timestamp_sec = total_frames as f64 / info.sample_rate as f64;
                if let Ok(seg) = bytes_to_segment(
                    &pending[..frames * frame_bytes],
                    &info,
                    frames,
                    chunk_index,
                    timestamp_sec,
                ) {
                    let _ = tx.send(seg).await;
                }
            }
        }

        info!(
            "UriSource: EOF after {} frames ({:.1}s)",
            total_frames,
            total_frames as f64 / info.sample_rate.max(1) as f64
        );
        Ok(true)
    }

    // ── Constructeurs internes ────────────────────────────────────────────────

    async fn open_file(path: &str, seek_sec: f64) -> Result<Self, AudioError> {
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|e| AudioError::IoError(format!("Cannot open {:?}: {}", path, e)))?;

        let stream = decode_audio_stream(file)
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Decode error: {}", e)))?;

        let stream_info = stream.info().clone();
        validate_stream(&stream_info)?;

        let frames_to_skip = (seek_sec * stream_info.sample_rate as f64) as u64;

        info!(
            "UriSource: opened file {} Hz {} ch {} bps {:.1}s",
            stream_info.sample_rate,
            stream_info.channels,
            stream_info.bits_per_sample,
            stream_info.total_samples
                .map(|s| s as f64 / stream_info.sample_rate as f64)
                .unwrap_or(0.0),
        );

        let (_, reader) = stream.into_reader();
        Ok(Self { reader: Box::new(reader), stream_info, frames_to_skip, is_continuous: false })
    }

    async fn open_http(
        url: &str,
        seek_sec: f64,
        stop_token: &CancellationToken,
    ) -> Result<Self, AudioError> {
        // Détecter si c'est un flux continu (radio, stream) basé sur l'URL
        let is_continuous = detect_continuous_stream(url);

        let response = tokio::select! {
            _ = stop_token.cancelled() => {
                return Err(AudioError::IoError("Cancelled before HTTP connect".into()));
            }
            result = reqwest::get(url) => {
                result.map_err(|e| AudioError::IoError(format!("HTTP error: {}", e)))?
            }
        };

        if !response.status().is_success() {
            return Err(AudioError::IoError(format!(
                "HTTP {} for {}",
                response.status(),
                url
            )));
        }

        use futures::TryStreamExt;
        use tokio_util::io::StreamReader;

        let byte_stream = response
            .bytes_stream()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
        let reader = StreamReader::new(byte_stream);

        let stream = decode_audio_stream(reader)
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Decode error: {}", e)))?;

        let stream_info = stream.info().clone();
        validate_stream(&stream_info)?;

        let frames_to_skip = (seek_sec * stream_info.sample_rate as f64) as u64;

        info!(
            "UriSource: opened HTTP {} Hz {} ch {} bps continuous={}",
            stream_info.sample_rate, stream_info.channels, stream_info.bits_per_sample, is_continuous,
        );

        let (_, reader) = stream.into_reader();
        Ok(Self { 
            reader: Box::new(reader), 
            stream_info, 
            frames_to_skip,
            is_continuous,
        })
    }
}

/// Détecte si une URL correspond à un flux continu (radio, stream) sans durée définie.
///
/// Cette fonction:
/// 1. Vérifie les patterns d'URL connus (stream, live, radio, etc.)
/// 2. Fait une requête HTTP HEAD pour vérifier les headers (Content-Length, ICY, etc.)
fn detect_continuous_stream(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    
    // 1. Quick check sur les patterns d'URL très explicites
    // Ces patterns indiquent clairement un stream live
    if url_lower.contains("/live")
        || url_lower.contains("/radiolar")
        || url_lower.contains(".pls")
        || url_lower.contains(".m3u")
        || url_lower.contains("icy")
    {
        return true;
    }
    
    // 2. Vérification HTTP headers (le plus fiable)
    if url.starts_with("http://") || url.starts_with("https://") {
        if let Ok(is_stream) = check_http_stream_headers(url) {
            if is_stream {
                return true;
            }
        }
    }
    
    false
}

/// Vérifie les headers HTTP pour déterminer si c'est un stream
fn check_http_stream_headers(url: &str) -> Result<bool, String> {
    use std::time::Duration;
    
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(3))
        .build();
    
    let response = agent
        .head(url)
        .call()
        .map_err(|e| format!("HTTP HEAD failed: {}", e))?;
    
    // Headers ICY (Icecast/Shoutcast) = toujours un stream
    if response.header("icy-name").is_some()
        || response.header("icy-metaint").is_some()
    {
        return Ok(true);
    }
    
    // Pas de Content-Length = stream potentiel
    let has_content_length = response.header("content-length").is_some();
    
    // Transfer-Encoding: chunked = stream potentiel
    let is_chunked = response
        .header("transfer-encoding")
        .map(|v| v.to_lowercase().contains("chunked"))
        .unwrap_or(false);
    
    // Decision: pas de Content-Length + (chunked ou content-type streaming)
    let content_type = response
        .header("content-type")
        .unwrap_or("")
        .to_lowercase();
    
    let is_streaming_mime = content_type.contains("audio/mpeg")
        || content_type.contains("audio/aac")
        || content_type.contains("application/ogg");
    
    Ok(!has_content_length && (is_streaming_mime || is_chunked))
}
