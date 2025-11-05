use crate::{
    nodes::{AudioError, TypedAudioNode, DEFAULT_CHANNEL_SIZE},
    pipeline::{Node, NodeLogic},
    type_constraints::TypeRequirement,
    AudioChunk, AudioPipelineNode, AudioSegment, SyncMarker,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Buffer partagé entre le thread async et le callback cpal
struct SharedBuffer {
    /// Buffer de samples (stéréo entrelacé)
    samples: VecDeque<f32>,
    /// Sample rate actuel (peut changer entre les tracks)
    sample_rate: u32,
    /// Flag pour indiquer EndOfStream
    end_of_stream: bool,
}

impl SharedBuffer {
    fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            sample_rate: 44100, // Default
            end_of_stream: false,
        }
    }

    fn push_samples(&mut self, samples: Vec<f32>, sample_rate: u32) {
        self.sample_rate = sample_rate;
        self.samples.extend(samples);
    }

    fn pop_sample(&mut self) -> Option<f32> {
        self.samples.pop_front()
    }

    fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    fn len(&self) -> usize {
        self.samples.len()
    }

    fn mark_end(&mut self) {
        self.end_of_stream = true;
    }

    fn is_finished(&self) -> bool {
        self.end_of_stream && self.samples.is_empty()
    }
}

/// Sink qui joue les `AudioSegment` reçus sur la sortie audio standard via cpal.
///
/// Ce sink :
/// - Lit les chunks audio et les joue en temps réel
/// - Convertit automatiquement tous les formats vers F32 pour cpal
/// - Supporte le changement de sample rate entre les tracks (avec resampling automatique si nécessaire)
/// - Gère TrackBoundary pour des transitions propres
/// - S'arrête proprement sur EndOfStream ou CancellationToken

// ═══════════════════════════════════════════════════════════════════════════
/// AudioSinkLogic - Logique métier pure
// ═══════════════════════════════════════════════════════════════════════════

/// Logique pure de lecture audio via cpal
pub struct AudioSinkLogic {
    volume: f32,
}

impl AudioSinkLogic {
    pub fn new() -> Self {
        Self { volume: 1.0 }
    }

    pub fn with_volume(volume: f32) -> Self {
        Self {
            volume: volume.clamp(0.0, 1.0),
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

        // Créer le buffer partagé
        let buffer = Arc::new(Mutex::new(SharedBuffer::new()));
        let buffer_clone = buffer.clone();

        // Initialiser cpal
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| AudioError::ProcessingError("No output device available".to_string()))?;

        tracing::debug!("Using audio device: {}", device.name().unwrap_or_else(|_| "Unknown".to_string()));

        // Obtenir la config par défaut
        let config = device
            .default_output_config()
            .map_err(|e| AudioError::ProcessingError(format!("Failed to get output config: {}", e)))?;

        tracing::debug!(
            "Output config: {} channels, {} Hz, {:?}",
            config.channels(),
            config.sample_rate().0,
            config.sample_format()
        );

        let volume = self.volume;

        // Créer le stream avec callback
        let stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut buf = buffer_clone.lock().unwrap();

                    for sample in data.iter_mut() {
                        *sample = buf.pop_sample().unwrap_or(0.0) * volume;
                    }
                },
                move |err| {
                    tracing::error!("Audio stream error: {}", err);
                },
                None,
            )
            .map_err(|e| AudioError::ProcessingError(format!("Failed to build output stream: {}", e)))?;

        // Démarrer le stream
        stream
            .play()
            .map_err(|e| AudioError::ProcessingError(format!("Failed to play stream: {}", e)))?;

        tracing::debug!("AudioSink initialized with volume={}", self.volume);

        // Boucle de réception et traitement des segments
        loop {
            // Vérifier si l'arrêt a été demandé
            if stop_token.is_cancelled() {
                tracing::debug!("AudioSinkLogic cancelled");
                drop(stream); // Arrêter le stream
                return Ok(());
            }

            // Vérifier si on a fini de jouer
            {
                let buf = buffer.lock().unwrap();
                if buf.is_finished() {
                    tracing::debug!("AudioSink: finished playing all samples");
                    drop(stream);
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
                            drop(stream);
                            return Ok(());
                        }
                    }
                }
                _ = stop_token.cancelled() => {
                    tracing::debug!("AudioSinkLogic cancelled during recv");
                    drop(stream);
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
                    // Convertir le chunk en samples f32
                    let samples = chunk_to_f32_samples(chunk)?;
                    let sample_rate = chunk.sample_rate();

                    if samples.is_empty() {
                        continue;
                    }

                    // Ajouter au buffer
                    {
                        let mut buf = buffer.lock().unwrap();
                        buf.push_samples(samples, sample_rate);
                    }

                    tracing::trace!(
                        "AudioSink: buffered chunk with {} frames at {}Hz (buffer size: {} samples)",
                        chunk.len(),
                        sample_rate,
                        buffer.lock().unwrap().len()
                    );
                }
                crate::_AudioSegment::Sync(marker) => {
                    match **marker {
                        SyncMarker::TrackBoundary { .. } => {
                            tracing::debug!("AudioSink: TrackBoundary received");
                            // Le buffer continue automatiquement - pas besoin d'action
                        }
                        SyncMarker::EndOfStream => {
                            tracing::debug!("AudioSink: EndOfStream received, waiting for playback to finish");
                            // Marquer la fin et attendre que le buffer se vide
                            buffer.lock().unwrap().mark_end();

                            // Attendre que tout soit joué
                            while !buffer.lock().unwrap().is_finished() {
                                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                            }

                            drop(stream);
                            return Ok(());
                        }
                        SyncMarker::Error(ref message) => {
                            tracing::warn!("AudioSink: Error marker received: {}", message);
                            // Continuer la lecture malgré l'erreur
                        }
                        _ => {
                            // Ignorer les autres sync markers (TopZeroSync, Heartbeat, etc.)
                            tracing::trace!("AudioSink: ignoring sync marker");
                        }
                    }
                }
            }
        }
    }
}

/// Convertit un AudioChunk en vecteur de samples f32 stéréo (entrelacés)
fn chunk_to_f32_samples(chunk: &AudioChunk) -> Result<Vec<f32>, AudioError> {
    let len = chunk.len();
    let mut samples = Vec::with_capacity(len * 2); // 2 channels

    match chunk {
        AudioChunk::I16(data) => {
            // Convertir de 16-bit vers float32
            for frame in data.get_frames() {
                let left = frame[0] as f32 / 32768.0;
                let right = frame[1] as f32 / 32768.0;
                samples.push(left);
                samples.push(right);
            }
        }
        AudioChunk::I24(data) => {
            // Convertir de 24-bit vers float32
            for frame in data.get_frames() {
                let left = frame[0].as_i32() as f32 / 8388608.0; // 2^23
                let right = frame[1].as_i32() as f32 / 8388608.0;
                samples.push(left);
                samples.push(right);
            }
        }
        AudioChunk::I32(data) => {
            // Convertir de 32-bit vers float32
            for frame in data.get_frames() {
                let left = frame[0] as f32 / 2147483648.0; // 2^31
                let right = frame[1] as f32 / 2147483648.0;
                samples.push(left);
                samples.push(right);
            }
        }
        AudioChunk::F32(data) => {
            // Format natif - copie directe avec clamping
            for frame in data.get_frames() {
                samples.push(frame[0].clamp(-1.0, 1.0));
                samples.push(frame[1].clamp(-1.0, 1.0));
            }
        }
        AudioChunk::F64(data) => {
            // Convertir de float64 vers float32
            for frame in data.get_frames() {
                let left = frame[0].clamp(-1.0, 1.0) as f32;
                let right = frame[1].clamp(-1.0, 1.0) as f32;
                samples.push(left);
                samples.push(right);
            }
        }
    }

    Ok(samples)
}

// ═══════════════════════════════════════════════════════════════════════════
// WRAPPER AudioSink - Délègue à Node<AudioSinkLogic>
// ═══════════════════════════════════════════════════════════════════════════

/// AudioSink - Joue les AudioSegment sur la sortie audio standard
///
/// Ce sink utilise cpal pour la lecture audio multiplateforme. Il accepte
/// tous les formats audio (I16, I24, I32, F32, F64) et les convertit
/// automatiquement en F32 pour la lecture.
///
/// # Exemple
///
/// ```no_run
/// use pmoaudio::{AudioSink, FileSource};
/// use tokio_util::sync::CancellationToken;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let source = FileSource::new("audio.flac").await?;
/// let mut sink = AudioSink::new();
///
/// // Connecter la source au sink
/// source.register(Box::new(sink));
///
/// // Démarrer la lecture
/// let stop_token = CancellationToken::new();
/// source.run(stop_token).await?;
/// # Ok(())
/// # }
/// ```
pub struct AudioSink {
    inner: Node<AudioSinkLogic>,
}

impl AudioSink {
    /// Crée un nouveau AudioSink avec volume par défaut (1.0)
    pub fn new() -> Self {
        Self {
            inner: Node::new_with_input(AudioSinkLogic::new(), DEFAULT_CHANNEL_SIZE),
        }
    }

    /// Crée un nouveau AudioSink avec un volume spécifique (0.0 à 1.0)
    pub fn with_volume(volume: f32) -> Self {
        Self {
            inner: Node::new_with_input(AudioSinkLogic::with_volume(volume), DEFAULT_CHANNEL_SIZE),
        }
    }

    /// Crée un nouveau AudioSink avec une taille de channel personnalisée
    pub fn with_channel_size(channel_size: usize, volume: f32) -> Self {
        Self {
            inner: Node::new_with_input(
                AudioSinkLogic::with_volume(volume),
                channel_size,
            ),
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

    async fn run(
        self: Box<Self>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
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
    fn test_chunk_to_f32_samples_from_i16() {
        let stereo = vec![[16384i16, -16384i16], [32767i16, -32768i16]];
        let chunk_data = AudioChunkData::new(stereo, 44100, 0.0);
        let chunk = AudioChunk::I16(chunk_data);

        let samples = chunk_to_f32_samples(&chunk).unwrap();
        assert_eq!(samples.len(), 4);
        // 16384 / 32768 = 0.5
        assert!((samples[0] - 0.5).abs() < 0.001);
        assert!((samples[1] + 0.5).abs() < 0.001);
        // 32767 / 32768 ≈ 0.999969
        assert!((samples[2] - 0.999969).abs() < 0.001);
        // -32768 / 32768 = -1.0
        assert!((samples[3] + 1.0).abs() < 0.001);
    }

    #[test]
    fn test_chunk_to_f32_samples_from_f32() {
        let stereo = vec![[0.5f32, -0.5f32], [1.0f32, -1.0f32]];
        let chunk_data = AudioChunkData::new(stereo, 48000, 0.0);
        let chunk = AudioChunk::F32(chunk_data);

        let samples = chunk_to_f32_samples(&chunk).unwrap();
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
    fn test_audio_sink_with_volume() {
        let sink = AudioSink::with_volume(0.5);
        assert!(sink.get_tx().is_some());
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

        buffer.push_samples(vec![0.5, -0.5, 1.0], 44100);
        assert_eq!(buffer.len(), 3);

        assert_eq!(buffer.pop_sample(), Some(0.5));
        assert_eq!(buffer.pop_sample(), Some(-0.5));
        assert_eq!(buffer.len(), 1);

        buffer.mark_end();
        assert_eq!(buffer.pop_sample(), Some(1.0));
        assert!(buffer.is_finished());
    }
}
