use crate::{
    nodes::{AudioError, TypedAudioNode, DEFAULT_CHUNK_DURATION_MS},
    pipeline::{Node, NodeLogic},
    type_constraints::TypeRequirement,
    AudioChunk, AudioChunkData, AudioPipelineNode, AudioSegment, I24,
};
use futures_util::StreamExt;
use pmoflac::{decode_audio_stream, StreamInfo};
use pmometadata::{MemoryTrackMetadata, TrackMetadata};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::{io::StreamReader, sync::CancellationToken};

/// HttpSource - Récupère un fichier audio via HTTP et publie des `AudioSegment`
///
/// Cette source télécharge un fichier audio depuis une URL HTTP/HTTPS,
/// utilise `pmoflac` pour le décoder (FLAC/MP3/OGG/WAV/AIFF) puis transforme
/// les échantillons PCM en `AudioSegment` stéréo avec le type approprié.
///
/// Le node émet trois types de syncmarkers :
/// - `TopZeroSync` au début du flux
/// - `TrackBoundary` avec les métadonnées extraites des headers HTTP
/// - `EndOfStream` à la fin du flux
///
/// # Métadonnées HTTP
///
/// Les métadonnées suivantes sont extraites des headers HTTP lorsqu'elles sont disponibles:
/// - `icy-name`: nom du stream (Icecast/Shoutcast) → utilisé comme titre
/// - `icy-url`: URL du stream source
/// - `content-type`: type MIME du contenu (ex: audio/flac, audio/mpeg)
///
/// Si aucun header `icy-name` n'est présent, le nom du fichier est extrait de l'URL
/// et utilisé comme titre.
///
/// # Exemples
///
/// ## Lecture d'un fichier FLAC distant
///
/// ```no_run
/// use pmoaudio::HttpSource;
/// use tokio::sync::mpsc;
///
/// #[tokio::main]
/// async fn main() {
///     let mut source = HttpSource::new("http://example.com/audio.flac");
///     let (tx, mut rx) = mpsc::channel(16);
///     source.add_subscriber(tx);
///
///     // Lancer la lecture dans une tâche séparée
///     tokio::spawn(async move {
///         source.run().await.unwrap();
///     });
///
///     // Recevoir et traiter les segments audio
///     while let Some(segment) = rx.recv().await {
///         if segment.is_audio_chunk() {
///             println!("Chunk reçu à {}s", segment.timestamp_sec);
///         }
///     }
/// }
/// ```
///
/// ## Stream Icecast/Shoutcast
///
/// ```no_run
/// use pmoaudio::HttpSource;
/// use tokio::sync::mpsc;
///
/// #[tokio::main]
/// async fn main() {
///     // Les métadonnées icy-name seront extraites automatiquement
///     let mut source = HttpSource::new("http://stream.example.com:8000/stream");
///     let (tx, rx) = mpsc::channel(32);
///     source.add_subscriber(tx);
///
///     tokio::spawn(async move {
///         source.run().await.unwrap();
///     });
/// }
/// ```
///
/// # Gestion des erreurs
///
/// La méthode `run()` peut retourner les erreurs suivantes:
/// - `AudioError::ProcessingError`: échec de connexion HTTP, status code non-200,
///   erreur de décodage audio, ou format non supporté
///
/// # Performance
///
/// - Le téléchargement et le décodage sont effectués en streaming
/// - Pas de buffering complet du fichier en mémoire
/// - La taille des chunks audio est calculée automatiquement pour ~50ms de latence
/// - Compatible avec les streams infinis (radios web, etc.)

// ═══════════════════════════════════════════════════════════════════════════
// HttpSourceLogic - Logique métier pure
// ═══════════════════════════════════════════════════════════════════════════

/// Logique pure de lecture HTTP et décodage audio
pub struct HttpSourceLogic {
    url: String,
    chunk_frames: usize,
}

impl HttpSourceLogic {
    pub fn new<S: Into<String>>(url: S, chunk_frames: usize) -> Self {
        Self {
            url: url.into(),
            chunk_frames,
        }
    }

    pub fn get_url(&self) -> String {
        self.url.clone()
    }

    pub fn get_chunc_frames(&self) -> usize  {
        self.chunk_frames
    }
}

#[async_trait::async_trait]
impl NodeLogic for HttpSourceLogic {
    async fn process(
        &mut self,
        _input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        macro_rules! send_to_children {
            ($segment:expr) => {
                for tx in &output {
                    tx.send($segment.clone())
                        .await
                        .map_err(|_| AudioError::ChildDied)?;
                }
            };
        }

        // Effectuer la requête HTTP
        let response = reqwest::get(&self.url)
            .await
            .map_err(|e| {
                AudioError::ProcessingError(format!("HTTP request failed for {}: {}", self.url, e))
            })?;

        // Vérifier le status
        if !response.status().is_success() {
            return Err(AudioError::ProcessingError(format!(
                "HTTP request returned status {}: {}",
                response.status(),
                self.url
            )));
        }

        // Extraire les métadonnées depuis les headers HTTP
        let metadata = extract_metadata_from_headers(&response, &self.url).await;

        // Convertir le stream de bytes en AsyncRead
        let bytes_stream = response.bytes_stream();
        let stream_reader = StreamReader::new(bytes_stream.map(|result| {
            result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        }));

        // Décoder le flux audio
        let mut stream = decode_audio_stream(stream_reader)
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Decode error: {}", e)))?;
        let stream_info = stream.info().clone();

        validate_stream(&stream_info)?;

        // Calculer la taille des chunks si non spécifiée (0 = auto)
        let chunk_frames_final = if self.chunk_frames == 0 {
            let frames =
                (stream_info.sample_rate as f64 * DEFAULT_CHUNK_DURATION_MS / 1000.0) as usize;
            frames.next_power_of_two().max(256)
        } else {
            self.chunk_frames.max(1)
        };

        // Émettre TopZeroSync
        send_to_children!(AudioSegment::new_top_zero_sync());

        // Émettre TrackBoundary avec les métadonnées HTTP
        let track_boundary = AudioSegment::new_track_boundary(
            0,
            0.0,
            Arc::new(tokio::sync::RwLock::new(metadata)),
        );
        send_to_children!(track_boundary);

        // Préparer la lecture des chunks audio
        let frame_bytes = stream_info.bytes_per_sample() * stream_info.channels as usize;
        let chunk_byte_len = chunk_frames_final * frame_bytes;
        let mut pending = Vec::new();
        let mut read_buf = vec![0u8; frame_bytes * 512.max(chunk_frames_final)];
        let mut chunk_index = 0u64;
        let mut total_frames = 0u64;

        // Lire et émettre les chunks audio
        loop {
            // Vérifier l'arrêt
            if stop_token.is_cancelled() {
                return Ok(());
            }

            // Remplir le buffer
            if pending.len() < chunk_byte_len {
                use tokio::io::AsyncReadExt;
                let read = tokio::select! {
                    result = stream.read(&mut read_buf) => {
                        result.map_err(|e| {
                            AudioError::ProcessingError(format!("I/O error while decoding: {}", e))
                        })?
                    }
                    _ = stop_token.cancelled() => {
                        return Ok(());
                    }
                };
                if read == 0 {
                    break;
                }
                pending.extend_from_slice(&read_buf[..read]);
            }

            if pending.is_empty() {
                break;
            }

            // Extraire un chunk
            let frames_in_pending = pending.len() / frame_bytes;
            let frames_to_emit = frames_in_pending.min(chunk_frames_final);
            let take_bytes = frames_to_emit * frame_bytes;
            let chunk_bytes = pending.drain(..take_bytes).collect::<Vec<u8>>();

            // Calculer le timestamp
            let timestamp_sec = total_frames as f64 / stream_info.sample_rate as f64;

            // Créer le segment audio
            let segment = bytes_to_segment(
                &chunk_bytes,
                &stream_info,
                frames_to_emit,
                chunk_index,
                timestamp_sec,
            )?;

            send_to_children!(segment);

            chunk_index += 1;
            total_frames += frames_to_emit as u64;
        }

        // Traiter le reste éventuel
        if !pending.is_empty() {
            let frames = pending.len() / frame_bytes;
            if frames > 0 {
                let timestamp_sec = total_frames as f64 / stream_info.sample_rate as f64;
                let segment =
                    bytes_to_segment(&pending, &stream_info, frames, chunk_index, timestamp_sec)?;
                send_to_children!(segment);
                total_frames += frames as u64;
                chunk_index += 1;
            }
        }

        // Émettre EndOfStream
        let final_timestamp = total_frames as f64 / stream_info.sample_rate as f64;
        let eos = AudioSegment::new_end_of_stream(chunk_index, final_timestamp);
        send_to_children!(eos);

        // Attendre la fin du décodage
        stream
            .wait()
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Decode task failed: {}", e)))?;

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HttpSource - Wrapper utilisant Node<HttpSourceLogic>
// ═══════════════════════════════════════════════════════════════════════════

pub struct HttpSource {
    inner: Node<HttpSourceLogic>,
}

impl HttpSource {
    /// Crée une nouvelle source HTTP avec calcul automatique de la taille des chunks.
    ///
    /// La taille des chunks sera calculée automatiquement pour obtenir environ 50ms
    /// de latence par chunk, en fonction du sample rate du fichier distant.
    ///
    /// # Arguments
    ///
    /// * `url` - URL HTTP ou HTTPS du fichier audio à télécharger
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use pmoaudio::HttpSource;
    ///
    /// let source = HttpSource::new("http://example.com/music.flac");
    /// ```
    pub fn new<S: Into<String>>(url: S) -> Self {
        Self::with_chunk_size(url, 0)
    }

    /// Crée une nouvelle source HTTP avec une taille de chunk spécifique.
    ///
    /// # Arguments
    ///
    /// * `url` - URL HTTP ou HTTPS du fichier audio à télécharger
    /// * `chunk_frames` - nombre d'échantillons par canal par chunk (0 = auto-calcul)
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use pmoaudio::HttpSource;
    ///
    /// // Utiliser des chunks de 2048 frames
    /// let source = HttpSource::with_chunk_size("http://example.com/music.mp3", 2048);
    /// ```
    pub fn with_chunk_size<S: Into<String>>(url: S, chunk_frames: usize) -> Self {
        let logic = HttpSourceLogic::new(url.into(), chunk_frames);
        Self {
            inner: Node::new_source(logic),
        }
    }

    pub fn get_url(&self) -> String {
        self.inner.logic().get_url()
    }

    pub fn get_chunc_frames(&self) -> usize {
        self.inner.logic().get_chunc_frames()
    }
}

/// Extrait les métadonnées disponibles depuis les headers HTTP
async fn extract_metadata_from_headers(
    response: &reqwest::Response,
    url: &str,
) -> MemoryTrackMetadata {
    let mut metadata = MemoryTrackMetadata::new();
    let headers = response.headers();

    // Icecast/Shoutcast stream name
    if let Some(name) = headers.get("icy-name").and_then(|v| v.to_str().ok()) {
        let _ = metadata.set_title(Some(name.to_string())).await;
    }

    // Icecast/Shoutcast stream URL (peut être utilisé comme source)
    if let Some(stream_url) = headers.get("icy-url").and_then(|v| v.to_str().ok()) {
        // On pourrait stocker ça dans un champ custom si nécessaire
        eprintln!("Stream URL: {}", stream_url);
    }

    // Content-Type pour déterminer le format
    if let Some(content_type) = headers.get("content-type").and_then(|v| v.to_str().ok()) {
        eprintln!("Content-Type: {}", content_type);
        // On pourrait utiliser ça pour valider le format attendu
    }

    // Si aucune métadonnée spécifique n'est trouvée, utiliser l'URL comme titre
    if metadata.get_title().await.ok().flatten().is_none() {
        // Extraire le nom du fichier depuis l'URL
        if let Some(filename) = url.rsplit('/').next() {
            if !filename.is_empty() {
                let _ = metadata.set_title(Some(filename.to_string())).await;
            }
        }
    }

    metadata
}

fn validate_stream(info: &StreamInfo) -> Result<(), AudioError> {
    if !(1..=2).contains(&info.channels) {
        return Err(AudioError::ProcessingError(format!(
            "Unsupported channel count: {}",
            info.channels
        )));
    }
    match info.bits_per_sample {
        8 | 16 | 24 | 32 => Ok(()),
        other => Err(AudioError::ProcessingError(format!(
            "Unsupported bit depth: {}",
            other
        ))),
    }
}

/// Convertit des bytes PCM en AudioSegment avec le type approprié
fn bytes_to_segment(
    chunk_bytes: &[u8],
    info: &StreamInfo,
    frames: usize,
    order: u64,
    timestamp_sec: f64,
) -> Result<Arc<AudioSegment>, AudioError> {
    let bytes_per_sample = info.bytes_per_sample();
    let channels = info.channels as usize;
    let frame_bytes = bytes_per_sample * channels;

    // Créer le chunk du bon type selon la profondeur de bit
    let chunk = match info.bits_per_sample {
        16 => {
            // Type I16
            let mut stereo = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let base = frame_idx * frame_bytes;
                let l = i16::from_le_bytes(
                    chunk_bytes[base..base + bytes_per_sample]
                        .try_into()
                        .unwrap(),
                );
                let r = if channels == 1 {
                    l
                } else {
                    i16::from_le_bytes(
                        chunk_bytes[base + bytes_per_sample..base + 2 * bytes_per_sample]
                            .try_into()
                            .unwrap(),
                    )
                };
                stereo.push([l, r]);
            }
            let chunk_data = AudioChunkData::new(stereo, info.sample_rate, 0.0);
            AudioChunk::I16(chunk_data)
        }
        24 => {
            // Type I24
            let mut stereo = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let base = frame_idx * frame_bytes;
                let l_i32 = {
                    let mut buf = [0u8; 4];
                    buf[..3].copy_from_slice(&chunk_bytes[base..base + 3]);
                    // Sign extend
                    if chunk_bytes[base + 2] & 0x80 != 0 {
                        buf[3] = 0xFF;
                    }
                    i32::from_le_bytes(buf)
                };
                let l = I24::new(l_i32).ok_or_else(|| {
                    AudioError::ProcessingError(format!("Invalid I24 value: {}", l_i32))
                })?;

                let r = if channels == 1 {
                    l
                } else {
                    let r_i32 = {
                        let mut buf = [0u8; 4];
                        buf[..3].copy_from_slice(
                            &chunk_bytes[base + bytes_per_sample..base + bytes_per_sample + 3],
                        );
                        // Sign extend
                        if chunk_bytes[base + bytes_per_sample + 2] & 0x80 != 0 {
                            buf[3] = 0xFF;
                        }
                        i32::from_le_bytes(buf)
                    };
                    I24::new(r_i32).ok_or_else(|| {
                        AudioError::ProcessingError(format!("Invalid I24 value: {}", r_i32))
                    })?
                };
                stereo.push([l, r]);
            }
            let chunk_data = AudioChunkData::new(stereo, info.sample_rate, 0.0);
            AudioChunk::I24(chunk_data)
        }
        32 => {
            // Type I32
            let mut stereo = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let base = frame_idx * frame_bytes;
                let l = i32::from_le_bytes(
                    chunk_bytes[base..base + bytes_per_sample]
                        .try_into()
                        .unwrap(),
                );
                let r = if channels == 1 {
                    l
                } else {
                    i32::from_le_bytes(
                        chunk_bytes[base + bytes_per_sample..base + 2 * bytes_per_sample]
                            .try_into()
                            .unwrap(),
                    )
                };
                stereo.push([l, r]);
            }
            let chunk_data = AudioChunkData::new(stereo, info.sample_rate, 0.0);
            AudioChunk::I32(chunk_data)
        }
        other => {
            return Err(AudioError::ProcessingError(format!(
                "Unsupported bit depth: {}",
                other
            )))
        }
    };

    // Créer le segment audio
    Ok(Arc::new(AudioSegment {
        order,
        timestamp_sec,
        segment: crate::_AudioSegment::Chunk(Arc::new(chunk)),
    }))
}

#[async_trait::async_trait]
impl AudioPipelineNode for HttpSource {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, child: Box<dyn AudioPipelineNode>) {
        self.inner.register(child)
    }

    async fn run(
        self: Box<Self>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }
}

impl TypedAudioNode for HttpSource {
    fn input_type(&self) -> Option<TypeRequirement> {
        // HttpSource est une source, elle ne consomme pas d'audio
        None
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        // HttpSource peut produire n'importe quel type entier (I16, I24, I32)
        // selon la profondeur de bit du fichier source
        Some(TypeRequirement::any_integer())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pmoflac::{encode_flac_stream, EncoderOptions, PcmFormat};
    use std::io::Cursor;
    use tokio::sync::mpsc;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    /// Nœud de test qui collecte tous les segments et les envoie à un channel de test
    struct TestCollectorNode {
        input_tx: mpsc::Sender<Arc<AudioSegment>>,
        input_rx: mpsc::Receiver<Arc<AudioSegment>>,
        output_tx: mpsc::Sender<Arc<AudioSegment>>,
    }

    impl TestCollectorNode {
        fn new(output_tx: mpsc::Sender<Arc<AudioSegment>>) -> Self {
            let (input_tx, input_rx) = mpsc::channel(16);
            Self {
                input_tx,
                input_rx,
                output_tx,
            }
        }
    }

    #[async_trait::async_trait]
    impl AudioPipelineNode for TestCollectorNode {
        fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
            Some(self.input_tx.clone())
        }

        fn register(&mut self, _child: Box<dyn AudioPipelineNode>) {
            panic!("TestCollectorNode is a sink and cannot have children");
        }

        async fn run(
            mut self: Box<Self>,
            _stop_token: CancellationToken,
        ) -> Result<(), AudioError> {
            while let Some(segment) = self.input_rx.recv().await {
                if self.output_tx.send(segment).await.is_err() {
                    break;
                }
            }
            Ok(())
        }
    }

    /// Test de création basique de HttpSource
    #[test]
    fn test_http_source_creation() {
        let source = HttpSource::new("http://example.com/audio.flac");
        assert_eq!(source.get_url(), "http://example.com/audio.flac");
        assert_eq!(source.get_chunc_frames(), 0);
    }

    /// Test de création avec taille de chunk personnalisée
    #[test]
    fn test_http_source_with_chunk_size() {
        let source = HttpSource::with_chunk_size("http://example.com/audio.mp3", 1024);
        assert_eq!(source.get_url(), "http://example.com/audio.mp3");
        assert_eq!(source.get_chunc_frames(), 1024);
    }

    /// Test de téléchargement et décodage d'un fichier FLAC via HTTP
    #[tokio::test]
    async fn test_http_source_downloads_and_decodes_flac() {
        // Créer un serveur HTTP mock
        let mock_server = MockServer::start().await;

        // Générer un petit fichier FLAC de test
        let sample_rate = 48_000;
        let frames = 256;
        let mut pcm = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let sample = ((i % 32) as f32 / 31.0 * 2.0 - 1.0) * 0.5;
            let sample_i16 = (sample * 32767.0) as i16;
            pcm.extend_from_slice(&sample_i16.to_le_bytes());
            pcm.extend_from_slice(&sample_i16.to_le_bytes());
        }

        let format = PcmFormat {
            sample_rate,
            channels: 2,
            bits_per_sample: 16,
        };

        let mut flac_stream =
            encode_flac_stream(Cursor::new(pcm.clone()), format, EncoderOptions::default())
                .await
                .unwrap();

        // Lire le FLAC encodé dans un buffer
        let mut flac_data = Vec::new();
        tokio::io::copy(&mut flac_stream, &mut flac_data)
            .await
            .unwrap();
        flac_stream.wait().await.unwrap();

        // Configurer le mock pour servir le fichier FLAC
        Mock::given(method("GET"))
            .and(path("/test.flac"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(flac_data)
                    .insert_header("content-type", "audio/flac"),
            )
            .mount(&mock_server)
            .await;

        // Créer la source HTTP pointant vers le mock
        let url = format!("{}/test.flac", mock_server.uri());
        let mut source = HttpSource::with_chunk_size(&url, 64);

        // Créer un noeud collecteur pour recevoir les segments
        let (tx, mut rx) = mpsc::channel(16);
        let collector = TestCollectorNode::new(tx);

        // Construire le pipeline
        source.register(Box::new(collector));

        // Lancer le pipeline
        let stop_token = CancellationToken::new();
        tokio::spawn(async move {
            Box::new(source).run(stop_token).await.unwrap();
        });

        // Vérifier les segments reçus
        let mut received_frames = 0usize;
        let mut seen_top_zero = false;
        let mut seen_track_boundary = false;
        let mut seen_eos = false;

        while let Some(segment) = rx.recv().await {
            if segment.is_audio_chunk() {
                if let Some(chunk) = segment.as_chunk() {
                    received_frames += chunk.len();
                    assert_eq!(chunk.sample_rate(), sample_rate);
                }
            } else if let Some(marker) = segment.as_sync_marker() {
                match **marker {
                    crate::SyncMarker::TopZeroSync => seen_top_zero = true,
                    crate::SyncMarker::TrackBoundary { .. } => seen_track_boundary = true,
                    crate::SyncMarker::EndOfStream => seen_eos = true,
                    _ => {}
                }
            }
        }

        // Vérifications
        assert_eq!(received_frames, frames, "Tous les frames doivent être reçus");
        assert!(seen_top_zero, "TopZeroSync doit être émis");
        assert!(seen_track_boundary, "TrackBoundary doit être émis");
        assert!(seen_eos, "EndOfStream doit être émis");
    }

    /// Test de l'extraction des métadonnées depuis les headers HTTP
    #[tokio::test]
    async fn test_http_source_extracts_icy_metadata() {
        let mock_server = MockServer::start().await;

        // Créer un fichier FLAC minimal
        let sample_rate = 48_000;
        let frames = 128;
        let mut pcm = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let sample_i16 = ((i % 100) as i16) * 100;
            pcm.extend_from_slice(&sample_i16.to_le_bytes());
            pcm.extend_from_slice(&sample_i16.to_le_bytes());
        }

        let format = PcmFormat {
            sample_rate,
            channels: 2,
            bits_per_sample: 16,
        };

        let mut flac_stream = encode_flac_stream(Cursor::new(pcm), format, EncoderOptions::default())
            .await
            .unwrap();

        let mut flac_data = Vec::new();
        tokio::io::copy(&mut flac_stream, &mut flac_data)
            .await
            .unwrap();
        flac_stream.wait().await.unwrap();

        // Configurer le mock avec headers Icecast
        Mock::given(method("GET"))
            .and(path("/stream"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(flac_data)
                    .insert_header("content-type", "audio/flac")
                    .insert_header("icy-name", "Test Radio Stream")
                    .insert_header("icy-url", "http://example.com/radio"),
            )
            .mount(&mock_server)
            .await;

        let url = format!("{}/stream", mock_server.uri());
        let mut source = HttpSource::new(&url);
        let (tx, mut rx) = mpsc::channel(16);
        let collector = TestCollectorNode::new(tx);
        source.register(Box::new(collector));

        let stop_token = CancellationToken::new();
        tokio::spawn(async move {
            Box::new(source).run(stop_token).await.unwrap();
        });

        // Chercher le TrackBoundary pour vérifier les métadonnées
        let mut found_metadata = false;
        while let Some(segment) = rx.recv().await {
            if let Some(marker) = segment.as_sync_marker() {
                if let crate::SyncMarker::TrackBoundary { metadata, .. } = &**marker {
                    // Vérifier que le titre extrait est "Test Radio Stream"
                    if let Some(title) = metadata.read().await.get_title().await.ok().flatten() {
                        assert_eq!(title, "Test Radio Stream");
                        found_metadata = true;
                    }
                }
            }
        }

        assert!(found_metadata, "Les métadonnées ICY doivent être extraites");
    }

    /// Test du comportement en cas d'erreur HTTP 404
    #[tokio::test]
    async fn test_http_source_handles_404_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/notfound.flac"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let url = format!("{}/notfound.flac", mock_server.uri());
        let mut source = HttpSource::new(&url);
        let (tx, _rx) = mpsc::channel(16);
        let collector = TestCollectorNode::new(tx);
        source.register(Box::new(collector));

        let stop_token = CancellationToken::new();
        let result = Box::new(source).run(stop_token).await;
        assert!(result.is_err(), "Doit retourner une erreur pour HTTP 404");

        if let Err(AudioError::ProcessingError(msg)) = result {
            assert!(msg.contains("404"), "Le message d'erreur doit mentionner le code 404");
        } else {
            panic!("Le type d'erreur doit être ProcessingError");
        }
    }

    /// Test du comportement avec un format audio invalide
    #[tokio::test]
    async fn test_http_source_handles_invalid_audio_format() {
        let mock_server = MockServer::start().await;

        // Envoyer des données invalides (pas un fichier audio)
        Mock::given(method("GET"))
            .and(path("/invalid.flac"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(b"This is not a valid audio file")
                    .insert_header("content-type", "audio/flac"),
            )
            .mount(&mock_server)
            .await;

        let url = format!("{}/invalid.flac", mock_server.uri());
        let mut source = HttpSource::new(&url);
        let (tx, _rx) = mpsc::channel(16);
        let collector = TestCollectorNode::new(tx);
        source.register(Box::new(collector));

        let stop_token = CancellationToken::new();
        let result = Box::new(source).run(stop_token).await;
        assert!(
            result.is_err(),
            "Doit retourner une erreur pour un format invalide"
        );
    }

    /// Test de l'extraction du nom de fichier depuis l'URL quand pas de header icy-name
    #[tokio::test]
    async fn test_http_source_uses_filename_as_title() {
        let mock_server = MockServer::start().await;

        let sample_rate = 48_000;
        let frames = 128;
        let mut pcm = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let sample_i16 = (i % 100) as i16;
            pcm.extend_from_slice(&sample_i16.to_le_bytes());
            pcm.extend_from_slice(&sample_i16.to_le_bytes());
        }

        let format = PcmFormat {
            sample_rate,
            channels: 2,
            bits_per_sample: 16,
        };

        let mut flac_stream = encode_flac_stream(Cursor::new(pcm), format, EncoderOptions::default())
            .await
            .unwrap();

        let mut flac_data = Vec::new();
        tokio::io::copy(&mut flac_stream, &mut flac_data)
            .await
            .unwrap();
        flac_stream.wait().await.unwrap();

        // Sans header icy-name
        Mock::given(method("GET"))
            .and(path("/my-song.flac"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(flac_data)
                    .insert_header("content-type", "audio/flac"),
            )
            .mount(&mock_server)
            .await;

        let url = format!("{}/my-song.flac", mock_server.uri());
        let mut source = HttpSource::new(&url);
        let (tx, mut rx) = mpsc::channel(16);
        let collector = TestCollectorNode::new(tx);
        source.register(Box::new(collector));

        let stop_token = CancellationToken::new();
        tokio::spawn(async move {
            Box::new(source).run(stop_token).await.unwrap();
        });

        let mut found_title = false;
        while let Some(segment) = rx.recv().await {
            if let Some(marker) = segment.as_sync_marker() {
                if let crate::SyncMarker::TrackBoundary { metadata, .. } = &**marker {
                    if let Some(title) = metadata.read().await.get_title().await.ok().flatten() {
                        assert_eq!(title, "my-song.flac");
                        found_title = true;
                    }
                }
            }
        }

        assert!(found_title, "Le nom du fichier doit être utilisé comme titre");
    }
}
