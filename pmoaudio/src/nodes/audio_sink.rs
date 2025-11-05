use crate::{
    nodes::{AudioError, TypedAudioNode, DEFAULT_CHANNEL_SIZE},
    pipeline::{Node, NodeLogic},
    type_constraints::TypeRequirement,
    AudioChunk, AudioPipelineNode, AudioSegment, SyncMarker,
};
use rodio::{OutputStream, Sink};
use std::sync::{
    mpsc as std_mpsc,
    Arc,
};
use std::thread;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Commandes envoyées au thread rodio
enum RodioCommand {
    AppendSamples {
        samples: Vec<i16>,
        sample_rate: u32,
    },
    WaitUntilEnd,
    Stop,
}

/// Sink qui joue les `AudioSegment` reçus sur la sortie audio standard via rodio.
///
/// Ce sink :
/// - Lit les chunks audio et les joue en temps réel
/// - Convertit automatiquement tous les formats vers I16 pour rodio
/// - Supporte le changement de sample rate entre les tracks
/// - Gère TrackBoundary pour des transitions propres
/// - S'arrête proprement sur EndOfStream ou CancellationToken

// ═══════════════════════════════════════════════════════════════════════════
/// AudioSinkLogic - Logique métier pure
// ═══════════════════════════════════════════════════════════════════════════

/// Logique pure de lecture audio via rodio
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

        // Créer un channel pour communiquer avec le thread rodio
        let (cmd_tx, cmd_rx) = std_mpsc::channel::<RodioCommand>();

        // Spawner un thread dédié pour rodio (car OutputStream n'est pas Send)
        let volume = self.volume;
        let rodio_thread = thread::spawn(move || {
            // Créer OutputStream et Sink dans le thread
            let (_stream, stream_handle) = match OutputStream::try_default() {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create audio output: {}", e);
                    return;
                }
            };

            let sink = match Sink::try_new(&stream_handle) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create sink: {}", e);
                    return;
                }
            };

            sink.set_volume(volume);
            tracing::debug!("Rodio thread initialized with volume={}", volume);

            // Boucle de traitement des commandes
            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    RodioCommand::AppendSamples { samples, sample_rate } => {
                        let buffer = rodio::buffer::SamplesBuffer::new(2, sample_rate, samples);
                        sink.append(buffer);
                    }
                    RodioCommand::WaitUntilEnd => {
                        sink.sleep_until_end();
                        break;
                    }
                    RodioCommand::Stop => {
                        sink.stop();
                        break;
                    }
                }
            }

            tracing::debug!("Rodio thread exiting");
        });

        tracing::debug!("AudioSink initialized with volume={}", self.volume);

        // Boucle de réception et traitement des segments
        loop {
            // Vérifier si l'arrêt a été demandé
            if stop_token.is_cancelled() {
                tracing::debug!("AudioSinkLogic cancelled");
                let _ = cmd_tx.send(RodioCommand::Stop);
                let _ = rodio_thread.join();
                return Ok(());
            }

            // Recevoir le prochain segment
            let segment = tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(seg) => seg,
                        None => {
                            tracing::debug!("AudioSinkLogic: input channel closed");
                            let _ = cmd_tx.send(RodioCommand::Stop);
                            let _ = rodio_thread.join();
                            return Ok(());
                        }
                    }
                }
                _ = stop_token.cancelled() => {
                    tracing::debug!("AudioSinkLogic cancelled during recv");
                    let _ = cmd_tx.send(RodioCommand::Stop);
                    let _ = rodio_thread.join();
                    return Ok(());
                }
            };

            // Traiter selon le type de segment
            match &segment.segment {
                crate::_AudioSegment::Chunk(chunk) => {
                    // Convertir le chunk en samples rodio
                    let samples = chunk_to_i16_samples(chunk)?;
                    let sample_rate = chunk.sample_rate();

                    if samples.is_empty() {
                        continue;
                    }

                    // Envoyer au thread rodio
                    cmd_tx
                        .send(RodioCommand::AppendSamples {
                            samples,
                            sample_rate,
                        })
                        .map_err(|_| {
                            AudioError::ProcessingError("Rodio thread died".to_string())
                        })?;

                    tracing::trace!(
                        "AudioSink: sent chunk with {} frames at {}Hz",
                        chunk.len(),
                        sample_rate
                    );
                }
                crate::_AudioSegment::Sync(marker) => {
                    match **marker {
                        SyncMarker::TrackBoundary { .. } => {
                            tracing::debug!("AudioSink: TrackBoundary received");
                            // Le sink continue automatiquement - pas besoin d'attendre
                            // Le buffer interne de rodio gère la transition
                        }
                        SyncMarker::EndOfStream => {
                            tracing::debug!("AudioSink: EndOfStream received, waiting for playback to finish");
                            // Demander au thread rodio d'attendre la fin
                            cmd_tx
                                .send(RodioCommand::WaitUntilEnd)
                                .map_err(|_| {
                                    AudioError::ProcessingError("Rodio thread died".to_string())
                                })?;
                            // Attendre que le thread termine
                            rodio_thread.join().map_err(|_| {
                                AudioError::ProcessingError("Failed to join rodio thread".to_string())
                            })?;
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

/// Convertit un AudioChunk en vecteur de samples i16 stéréo
fn chunk_to_i16_samples(chunk: &AudioChunk) -> Result<Vec<i16>, AudioError> {
    let len = chunk.len();
    let mut samples = Vec::with_capacity(len * 2); // 2 channels

    match chunk {
        AudioChunk::I16(data) => {
            // Format natif - copie directe
            for frame in data.get_frames() {
                samples.push(frame[0]);
                samples.push(frame[1]);
            }
        }
        AudioChunk::I24(data) => {
            // Convertir de 24-bit vers 16-bit
            for frame in data.get_frames() {
                let left = (frame[0].as_i32() >> 8) as i16;
                let right = (frame[1].as_i32() >> 8) as i16;
                samples.push(left);
                samples.push(right);
            }
        }
        AudioChunk::I32(data) => {
            // Convertir de 32-bit vers 16-bit
            for frame in data.get_frames() {
                let left = (frame[0] >> 16) as i16;
                let right = (frame[1] >> 16) as i16;
                samples.push(left);
                samples.push(right);
            }
        }
        AudioChunk::F32(data) => {
            // Convertir de float32 vers 16-bit
            for frame in data.get_frames() {
                let left = (frame[0].clamp(-1.0, 1.0) * 32767.0) as i16;
                let right = (frame[1].clamp(-1.0, 1.0) * 32767.0) as i16;
                samples.push(left);
                samples.push(right);
            }
        }
        AudioChunk::F64(data) => {
            // Convertir de float64 vers 16-bit
            for frame in data.get_frames() {
                let left = (frame[0].clamp(-1.0, 1.0) * 32767.0) as i16;
                let right = (frame[1].clamp(-1.0, 1.0) * 32767.0) as i16;
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
/// Ce sink utilise rodio pour la lecture audio multiplateforme. Il accepte
/// tous les formats audio (I16, I24, I32, F32, F64) et les convertit
/// automatiquement en I16 pour la lecture.
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
    fn test_chunk_to_i16_samples_from_i16() {
        let stereo = vec![[100i16, 200i16], [300i16, 400i16]];
        let chunk_data = AudioChunkData::new(stereo, 44100, 0.0);
        let chunk = AudioChunk::I16(chunk_data);

        let samples = chunk_to_i16_samples(&chunk).unwrap();
        assert_eq!(samples, vec![100, 200, 300, 400]);
    }

    #[test]
    fn test_chunk_to_i16_samples_from_f32() {
        use crate::I24;
        let stereo = vec![[0.5f32, -0.5f32], [1.0f32, -1.0f32]];
        let chunk_data = AudioChunkData::new(stereo, 48000, 0.0);
        let chunk = AudioChunk::F32(chunk_data);

        let samples = chunk_to_i16_samples(&chunk).unwrap();
        // 0.5 * 32767 ≈ 16383
        // -0.5 * 32767 ≈ -16383
        // 1.0 * 32767 = 32767
        // -1.0 * 32767 = -32767
        assert_eq!(samples.len(), 4);
        assert!((samples[0] - 16383).abs() <= 1);
        assert!((samples[1] + 16383).abs() <= 1);
        assert_eq!(samples[2], 32767);
        assert_eq!(samples[3], -32767);
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
}
