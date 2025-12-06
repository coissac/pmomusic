use crate::{
    dsp::{i16_stereo_to_pairs_f32, i24_as_i32_stereo_to_pairs_f32, i32_stereo_to_interleaved_f32},
    nodes::{AudioError, TypedAudioNode, DEFAULT_CHANNEL_SIZE},
    pipeline::{Node, NodeLogic},
    type_constraints::TypeRequirement,
    AudioChunk, AudioPipelineNode, AudioSegment, BitDepth, SyncMarker,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Buffer partagé entre le thread async et le callback cpal
/// Stocke les AudioChunk bruts et un buffer intermédiaire pour les samples convertis
struct SharedBuffer {
    /// Queue d'AudioChunk à traiter
    chunks: VecDeque<Arc<AudioChunk>>,
    /// Buffer intermédiaire de samples convertis au format hardware (entrelacé)
    converted_samples: VecDeque<f32>,
    /// Flag pour indiquer EndOfStream
    end_of_stream: bool,
}

impl SharedBuffer {
    fn new() -> Self {
        Self {
            chunks: VecDeque::new(),
            converted_samples: VecDeque::new(),
            end_of_stream: false,
        }
    }

    fn push_chunk(&mut self, chunk: Arc<AudioChunk>) {
        self.chunks.push_back(chunk);
    }

    /// Convertit le prochain chunk en samples F32 entrelacés (pour conversion ultérieure)
    fn convert_next_chunk_to_f32(&mut self) -> bool {
        if let Some(chunk) = self.chunks.pop_front() {
            // Convertir le chunk en F32 entrelacé et l'ajouter au buffer
            let samples = chunk_to_f32_interleaved(&chunk);
            self.converted_samples.extend(samples);
            true
        } else {
            false
        }
    }

    fn pop_sample_f32(&mut self) -> Option<f32> {
        if self.converted_samples.is_empty() {
            // Essayer de convertir le prochain chunk
            self.convert_next_chunk_to_f32();
        }
        self.converted_samples.pop_front()
    }

    fn is_empty(&self) -> bool {
        self.chunks.is_empty() && self.converted_samples.is_empty()
    }

    fn mark_end(&mut self) {
        self.end_of_stream = true;
    }

    fn is_finished(&self) -> bool {
        self.end_of_stream && self.is_empty()
    }
}

/// Convertit un AudioChunk en vecteur de samples f32 stéréo entrelacés [L, R, L, R, ...]
/// Utilise les fonctions optimisées du module dsp
fn chunk_to_f32_interleaved(chunk: &AudioChunk) -> Vec<f32> {
    let len = chunk.len();

    match chunk {
        AudioChunk::I16(data) => {
            // Utiliser la fonction optimisée SIMD
            let frames = data.get_frames();
            let mut left = Vec::with_capacity(len);
            let mut right = Vec::with_capacity(len);

            for frame in frames {
                left.push(frame[0]);
                right.push(frame[1]);
            }

            let mut out_pairs = vec![[0.0f32, 0.0f32]; len];
            i16_stereo_to_pairs_f32(&left, &right, &mut out_pairs);

            // Convertir en entrelacé
            let mut interleaved = Vec::with_capacity(len * 2);
            for pair in out_pairs {
                interleaved.push(pair[0]);
                interleaved.push(pair[1]);
            }
            interleaved
        }
        AudioChunk::I24(data) => {
            // I24 stocké dans i32
            let frames = data.get_frames();
            let mut left = Vec::with_capacity(len);
            let mut right = Vec::with_capacity(len);

            for frame in frames {
                left.push(frame[0].as_i32());
                right.push(frame[1].as_i32());
            }

            let mut out_pairs = vec![[0.0f32, 0.0f32]; len];
            i24_as_i32_stereo_to_pairs_f32(&left, &right, &mut out_pairs);

            // Convertir en entrelacé
            let mut interleaved = Vec::with_capacity(len * 2);
            for pair in out_pairs {
                interleaved.push(pair[0]);
                interleaved.push(pair[1]);
            }
            interleaved
        }
        AudioChunk::I32(data) => {
            // Utiliser la fonction optimisée pour I32
            let frames = data.get_frames();
            let mut left = Vec::with_capacity(len);
            let mut right = Vec::with_capacity(len);

            for frame in frames {
                left.push(frame[0]);
                right.push(frame[1]);
            }

            let mut out_interleaved = vec![0.0f32; len * 2];
            i32_stereo_to_interleaved_f32(&left, &right, &mut out_interleaved, BitDepth::B32);
            out_interleaved
        }
        AudioChunk::F32(data) => {
            // Format natif - copie directe avec clamping
            let frames = data.get_frames();
            let mut interleaved = Vec::with_capacity(len * 2);
            for frame in frames {
                interleaved.push(frame[0].clamp(-1.0, 1.0));
                interleaved.push(frame[1].clamp(-1.0, 1.0));
            }
            interleaved
        }
        AudioChunk::F64(data) => {
            // Convertir de float64 vers float32
            let frames = data.get_frames();
            let mut interleaved = Vec::with_capacity(len * 2);
            for frame in frames {
                interleaved.push(frame[0].clamp(-1.0, 1.0) as f32);
                interleaved.push(frame[1].clamp(-1.0, 1.0) as f32);
            }
            interleaved
        }
    }
}

/// Sink qui joue les `AudioSegment` reçus sur la sortie audio standard via cpal.
///
/// Ce sink :
/// - Détecte automatiquement le format hardware (I16, F32, U16)
/// - Accepte tous les formats AudioChunk en entrée
/// - Convertit en utilisant les fonctions optimisées SIMD du module dsp
/// - Gère TrackBoundary pour des transitions propres
/// - S'arrête proprement sur EndOfStream ou CancellationToken

// ═══════════════════════════════════════════════════════════════════════════
/// AudioSinkLogic - Logique métier pure
// ═══════════════════════════════════════════════════════════════════════════

/// Logique pure de lecture audio via cpal
pub struct AudioSinkLogic {
    use_null_output: bool,
}

impl AudioSinkLogic {
    pub fn new() -> Self {
        Self {
            use_null_output: false,
        }
    }

    pub fn with_null_output() -> Self {
        Self {
            use_null_output: true,
        }
    }

    /// Version null output - consomme les segments sans les jouer
    async fn process_null_output(
        mut rx: mpsc::Receiver<Arc<AudioSegment>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        loop {
            let segment = tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(seg) => seg,
                        None => {
                            tracing::debug!("AudioSinkLogic (null): input channel closed");
                            return Ok(());
                        }
                    }
                }
                _ = stop_token.cancelled() => {
                    tracing::debug!("AudioSinkLogic (null): cancelled");
                    return Ok(());
                }
            };

            // Juste logger les segments sans les jouer
            match &segment.segment {
                crate::_AudioSegment::Chunk(chunk) => {
                    tracing::trace!(
                        "AudioSink (null): consumed chunk with {} frames at {}Hz",
                        chunk.len(),
                        chunk.sample_rate()
                    );
                }
                crate::_AudioSegment::Sync(marker) => match **marker {
                    SyncMarker::TrackBoundary { .. } => {
                        tracing::debug!("AudioSink (null): TrackBoundary received");
                    }
                    SyncMarker::EndOfStream => {
                        tracing::debug!("AudioSink (null): EndOfStream received");
                        return Ok(());
                    }
                    _ => {
                        tracing::trace!("AudioSink (null): sync marker");
                    }
                },
            }
        }
    }
}

impl Default for AudioSinkLogic {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl NodeLogic for AudioSinkLogic {
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        _output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let mut rx = input.expect("AudioSink must have input");

        tracing::debug!("AudioSinkLogic::process started");

        // Si null output, juste consommer les segments sans jouer
        if self.use_null_output {
            tracing::debug!("Using null audio output (no playback)");
            return Self::process_null_output(rx, stop_token).await;
        }

        // Créer le buffer partagé
        let buffer = Arc::new(Mutex::new(SharedBuffer::new()));
        let buffer_clone = buffer.clone();

        // Initialiser cpal
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| AudioError::ProcessingError("No output device available".to_string()))?;

        tracing::debug!(
            "Using audio device: {}",
            device.name().unwrap_or_else(|_| "Unknown".to_string())
        );

        // Obtenir la config par défaut
        let config = device.default_output_config().map_err(|e| {
            AudioError::ProcessingError(format!("Failed to get output config: {}", e))
        })?;

        let sample_format = config.sample_format();
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        tracing::debug!(
            "Output config: {} channels, {} Hz, {:?}",
            channels,
            sample_rate,
            sample_format
        );

        // Créer un channel pour commander le thread du stream
        let (stream_cmd_tx, stream_cmd_rx) = std_mpsc::channel::<bool>();

        // Spawn un thread dédié pour le stream cpal (car Stream n'est pas Send)
        let stream_thread = thread::spawn(move || {
            // Créer le stream selon le format hardware
            let stream = match sample_format {
                cpal::SampleFormat::I16 => {
                    tracing::debug!("Using I16 output format");
                    match device.build_output_stream(
                        &config.into(),
                        move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                            let mut buf = buffer_clone.lock().unwrap();

                            // Remplir avec des samples convertis
                            for sample in data.iter_mut() {
                                let f32_sample = buf.pop_sample_f32().unwrap_or(0.0);
                                // Convertir F32 [-1.0, 1.0] → I16
                                *sample = (f32_sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                            }
                        },
                        move |err| {
                            tracing::error!("Audio stream error: {}", err);
                        },
                        None,
                    ) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("Failed to build I16 stream: {}", e);
                            return;
                        }
                    }
                }
                cpal::SampleFormat::U16 => {
                    tracing::debug!("Using U16 output format");
                    match device.build_output_stream(
                        &config.into(),
                        move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                            let mut buf = buffer_clone.lock().unwrap();

                            for sample in data.iter_mut() {
                                let f32_sample = buf.pop_sample_f32().unwrap_or(0.0);
                                // Convertir F32 [-1.0, 1.0] → U16 [0, 65535]
                                *sample = ((f32_sample + 1.0) * 32767.5).clamp(0.0, 65535.0) as u16;
                            }
                        },
                        move |err| {
                            tracing::error!("Audio stream error: {}", err);
                        },
                        None,
                    ) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("Failed to build U16 stream: {}", e);
                            return;
                        }
                    }
                }
                cpal::SampleFormat::F32 => {
                    tracing::debug!("Using F32 output format");
                    match device.build_output_stream(
                        &config.into(),
                        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                            let mut buf = buffer_clone.lock().unwrap();

                            for sample in data.iter_mut() {
                                *sample = buf.pop_sample_f32().unwrap_or(0.0);
                            }
                        },
                        move |err| {
                            tracing::error!("Audio stream error: {}", err);
                        },
                        None,
                    ) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("Failed to build F32 stream: {}", e);
                            return;
                        }
                    }
                }
                _ => {
                    tracing::error!("Unsupported sample format: {:?}", sample_format);
                    return;
                }
            };

            // Démarrer le stream
            if let Err(e) = stream.play() {
                tracing::error!("Failed to start stream: {}", e);
                return;
            }

            tracing::debug!("Stream thread started");

            // Attendre la commande d'arrêt
            let _ = stream_cmd_rx.recv();

            // Le stream se fermera automatiquement quand il sera droppé
            tracing::debug!("Stream thread exiting");
        });

        tracing::debug!("AudioSink initialized with format {:?}", sample_format);

        // Boucle de réception et traitement des segments
        loop {
            // Vérifier si l'arrêt a été demandé
            if stop_token.is_cancelled() {
                tracing::debug!("AudioSinkLogic cancelled");
                let _ = stream_cmd_tx.send(true);
                let _ = stream_thread.join();
                return Ok(());
            }

            // Vérifier si on a fini de jouer
            {
                let buf = buffer.lock().unwrap();
                if buf.is_finished() {
                    tracing::debug!("AudioSink: finished playing all samples");
                    let _ = stream_cmd_tx.send(true);
                    let _ = stream_thread.join();
                    return Ok(());
                }
            }

            // Recevoir le prochain segment (avec timeout pour vérifier périodiquement le buffer)
            let segment = tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(seg) => seg,
                        None => {
                            tracing::debug!("AudioSinkLogic: input channel closed");
                            // Attendre que le buffer se vide
                            while !buffer.lock().unwrap().is_empty() {
                                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                            }
                            let _ = stream_cmd_tx.send(true);
                            let _ = stream_thread.join();
                            return Ok(());
                        }
                    }
                }
                _ = stop_token.cancelled() => {
                    tracing::debug!("AudioSinkLogic cancelled during recv");
                    let _ = stream_cmd_tx.send(true);
                    let _ = stream_thread.join();
                    return Ok(());
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    // Timeout - vérifier le buffer et continuer
                    continue;
                }
            };

            // Traiter selon le type de segment
            match &segment.segment {
                crate::_AudioSegment::Chunk(chunk) => {
                    // Ajouter le chunk au buffer (pas de conversion ici)
                    {
                        let mut buf = buffer.lock().unwrap();
                        buf.push_chunk(chunk.clone());
                    }

                    tracing::trace!(
                        "AudioSink: buffered chunk with {} frames at {}Hz",
                        chunk.len(),
                        chunk.sample_rate()
                    );
                }
                crate::_AudioSegment::Sync(marker) => {
                    match **marker {
                        SyncMarker::TrackBoundary { .. } => {
                            tracing::debug!("AudioSink: TrackBoundary received");
                            // Le buffer continue automatiquement - pas besoin d'action
                        }
                        SyncMarker::EndOfStream => {
                            tracing::debug!(
                                "AudioSink: EndOfStream received, waiting for playback to finish"
                            );
                            // Marquer la fin et attendre que le buffer se vide
                            buffer.lock().unwrap().mark_end();

                            // Attendre que tout soit joué
                            while !buffer.lock().unwrap().is_finished() {
                                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                            }

                            let _ = stream_cmd_tx.send(true);
                            let _ = stream_thread.join();
                            return Ok(());
                        }
                        SyncMarker::Error(ref message) => {
                            tracing::warn!("AudioSink: Error marker received: {}", message);
                            // Continuer la lecture malgré l'erreur
                        }
                        _ => {
                            // Ignorer les autres sync markers
                            tracing::trace!("AudioSink: ignoring sync marker");
                        }
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WRAPPER AudioSink - Délègue à Node<AudioSinkLogic>
// ═══════════════════════════════════════════════════════════════════════════

/// AudioSink - Joue les AudioSegment sur la sortie audio standard
///
/// Ce sink utilise cpal pour la lecture audio multiplateforme. Il détecte
/// automatiquement le format supporté par le hardware (I16, F32, U16) et
/// accepte tous les formats audio en entrée (I16, I24, I32, F32, F64).
///
/// Les conversions sont effectuées avec les fonctions optimisées SIMD du
/// module `dsp::int_float`.
///
/// # Volume
///
/// Ce sink ne gère PAS le volume. Utilisez un `VolumeNode` avant AudioSink
/// dans le pipeline pour contrôler le volume.
///
/// # Exemple
///
/// ```no_run
/// use pmoaudio::{AudioSink, FileSource};
/// use tokio_util::sync::CancellationToken;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut source = FileSource::new("audio.flac").await?;
/// let sink = AudioSink::new();
///
/// // Connecter la source au sink
/// source.register(Box::new(sink));
///
/// // Démarrer la lecture
/// let stop_token = CancellationToken::new();
/// Box::new(source).run(stop_token).await?;
/// # Ok(())
/// # }
/// ```
pub struct AudioSink {
    inner: Node<AudioSinkLogic>,
}

impl AudioSink {
    /// Crée un nouveau AudioSink
    pub fn new() -> Self {
        Self {
            inner: Node::new_with_input(AudioSinkLogic::new(), DEFAULT_CHANNEL_SIZE),
        }
    }

    /// Crée un nouveau AudioSink avec une taille de channel personnalisée
    pub fn with_channel_size(channel_size: usize) -> Self {
        Self {
            inner: Node::new_with_input(AudioSinkLogic::new(), channel_size),
        }
    }

    /// Crée un AudioSink avec null output (pour tests sans carte audio)
    /// Consomme les segments audio sans les jouer
    pub fn with_null_output() -> Self {
        Self {
            inner: Node::new_with_input(AudioSinkLogic::with_null_output(), DEFAULT_CHANNEL_SIZE),
        }
    }
}

impl Default for AudioSink {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AudioPipelineNode for AudioSink {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, _child: Box<dyn AudioPipelineNode>) {
        panic!("AudioSink is a terminal node and cannot have children");
    }

    async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }
}

impl TypedAudioNode for AudioSink {
    fn input_type(&self) -> Option<TypeRequirement> {
        // AudioSink accepte tous les types audio
        Some(TypeRequirement::any())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        // AudioSink est un sink terminal - pas de sortie
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AudioChunkData;

    #[test]
    fn test_chunk_to_f32_interleaved_from_i16() {
        let stereo = vec![[16384i16, -16384i16], [32767i16, -32768i16]];
        let chunk_data = AudioChunkData::new(stereo, 44100, 0.0);
        let chunk = AudioChunk::I16(chunk_data);

        let samples = chunk_to_f32_interleaved(&chunk);
        assert_eq!(samples.len(), 4);
        // Vérifier que les valeurs sont normalisées
        assert!((samples[0] - 0.5).abs() < 0.01);
        assert!((samples[1] + 0.5).abs() < 0.01);
    }

    #[test]
    fn test_chunk_to_f32_interleaved_from_f32() {
        let stereo = vec![[0.5f32, -0.5f32], [1.0f32, -1.0f32]];
        let chunk_data = AudioChunkData::new(stereo, 48000, 0.0);
        let chunk = AudioChunk::F32(chunk_data);

        let samples = chunk_to_f32_interleaved(&chunk);
        assert_eq!(samples, vec![0.5, -0.5, 1.0, -1.0]);
    }

    #[test]
    fn test_audio_sink_creation() {
        let sink = AudioSink::new();
        assert!(sink.get_tx().is_some());
        assert!(sink.input_type().is_some());
        assert!(sink.output_type().is_none());
    }

    #[test]
    #[should_panic(expected = "terminal node")]
    fn test_audio_sink_cannot_have_children() {
        let mut sink = AudioSink::new();
        let another_sink = AudioSink::new();
        sink.register(Box::new(another_sink));
    }

    #[test]
    fn test_shared_buffer() {
        let mut buffer = SharedBuffer::new();

        assert!(buffer.is_empty());
        assert!(!buffer.is_finished());

        // Test avec un chunk F32
        let stereo = vec![[0.5f32, -0.5f32]];
        let chunk_data = AudioChunkData::new(stereo, 48000, 0.0);
        let chunk = Arc::new(AudioChunk::F32(chunk_data));

        buffer.push_chunk(chunk);
        assert!(!buffer.is_empty());

        // Pop quelques samples
        assert_eq!(buffer.pop_sample_f32(), Some(0.5));
        assert_eq!(buffer.pop_sample_f32(), Some(-0.5));
        assert_eq!(buffer.pop_sample_f32(), None);

        buffer.mark_end();
        assert!(buffer.is_finished());
    }
}
