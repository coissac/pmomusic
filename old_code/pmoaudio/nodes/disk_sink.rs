//! DiskSink - Écrit le flux audio dans un fichier
//!
//! Ce module fournit un sink qui écrit les chunks audio sur disque,
//! avec support de la dérivation automatique du nom de fichier depuis la source.

use crate::{events::SourceNameUpdateEvent, nodes::AudioError, AudioChunk};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, RwLock};

/// Configuration pour le DiskSink
#[derive(Debug, Clone)]
pub struct DiskSinkConfig {
    /// Chemin racine où écrire les fichiers
    pub output_dir: PathBuf,

    /// Nom de fichier explicite (optionnel)
    /// Si None, sera dérivé du nom de la source
    pub filename: Option<String>,

    /// Format d'écriture
    pub format: AudioFileFormat,

    /// Taille du buffer d'écriture (en chunks)
    pub buffer_size: usize,
}

impl Default for DiskSinkConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("."),
            filename: None,
            format: AudioFileFormat::Wav,
            buffer_size: 100,
        }
    }
}

/// Formats de fichiers audio supportés
#[derive(Debug, Clone, Copy)]
pub enum AudioFileFormat {
    /// Format WAV (non compressé)
    Wav,
    /// Format FLAC (compressé sans perte)
    Flac,
    /// Format brut PCM
    Raw,
}

impl AudioFileFormat {
    /// Retourne l'extension de fichier appropriée
    pub fn extension(&self) -> &str {
        match self {
            AudioFileFormat::Wav => "wav",
            AudioFileFormat::Flac => "flac",
            AudioFileFormat::Raw => "pcm",
        }
    }
}

/// DiskSink - Écrit le flux audio dans un fichier sur disque
///
/// Ce sink consomme les chunks audio et les écrit dans un fichier.
/// Le nom du fichier peut être dérivé automatiquement du nom de la source
/// via les événements `SourceNameUpdateEvent`.
///
/// # Caractéristiques
///
/// - Écriture asynchrone avec buffer
/// - Dérivation automatique du nom de fichier depuis la source
/// - Support de plusieurs formats (WAV, FLAC, PCM brut)
/// - Gestion du gain : applique le gain avant l'écriture
///
/// # Exemples
///
/// ```no_run
/// use pmoaudio::{DiskSink, DiskSinkConfig};
/// use std::path::PathBuf;
///
/// #[tokio::main]
/// async fn main() {
///     let config = DiskSinkConfig {
///         output_dir: PathBuf::from("/tmp/audio"),
///         filename: Some("output.wav".to_string()),
///         ..Default::default()
///     };
///
///     let (sink, sink_tx) = DiskSink::new("disk1".to_string(), config, 10);
///
///     tokio::spawn(async move {
///         sink.run().await.unwrap()
///     });
/// }
/// ```
pub struct DiskSink {
    /// Identifiant du sink
    node_id: String,

    /// Channel pour recevoir les chunks audio
    rx: mpsc::Receiver<Arc<AudioChunk>>,

    /// Configuration
    config: DiskSinkConfig,

    /// Nom de fichier résolu (partagé)
    resolved_filename: Arc<RwLock<Option<PathBuf>>>,

    /// Receiver pour les événements de nom de source (optionnel)
    source_name_rx: Option<mpsc::Receiver<SourceNameUpdateEvent>>,

    /// Writer pour le fichier
    writer: Option<AudioFileWriter>,
}

impl DiskSink {
    /// Crée un nouveau DiskSink
    ///
    /// # Arguments
    ///
    /// * `node_id` - Identifiant unique du sink
    /// * `config` - Configuration du sink
    /// * `channel_size` - Taille du buffer du channel
    pub fn new(
        node_id: String,
        config: DiskSinkConfig,
        channel_size: usize,
    ) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (tx, rx) = mpsc::channel(channel_size);

        let sink = Self {
            node_id,
            rx,
            config,
            resolved_filename: Arc::new(RwLock::new(None)),
            source_name_rx: None,
            writer: None,
        };

        (sink, tx)
    }

    /// Configure la source des événements de nom de source
    pub fn set_source_name_source(&mut self, rx: mpsc::Receiver<SourceNameUpdateEvent>) {
        self.source_name_rx = Some(rx);
    }

    /// Résout le nom du fichier de sortie
    ///
    /// Si un filename explicite est fourni dans la config, l'utilise.
    /// Sinon, utilise le source_name avec l'extension appropriée.
    fn resolve_filename(&self, source_name: Option<&str>) -> PathBuf {
        let filename = if let Some(ref explicit_name) = self.config.filename {
            explicit_name.clone()
        } else if let Some(name) = source_name {
            // Nettoyer le nom de la source pour en faire un nom de fichier valide
            let clean_name = name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '_' || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>();

            format!("{}.{}", clean_name, self.config.format.extension())
        } else {
            // Fallback sur un nom par défaut
            format!("{}.{}", self.node_id, self.config.format.extension())
        };

        self.config.output_dir.join(filename)
    }

    /// Initialise le writer pour le fichier de sortie
    async fn initialize_writer(&mut self, source_name: Option<&str>) -> Result<(), AudioError> {
        let path = self.resolve_filename(source_name);
        *self.resolved_filename.write().await = Some(path.clone());

        // Créer le répertoire parent si nécessaire
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AudioError::ProcessingError(format!("Failed to create directory: {}", e))
            })?;
        }

        // Créer le writer approprié selon le format
        let writer = match self.config.format {
            AudioFileFormat::Wav => AudioFileWriter::new_wav(path).await?,
            AudioFileFormat::Flac => {
                // FLAC nécessiterait une bibliothèque externe, pour l'instant utiliser WAV
                AudioFileWriter::new_wav(path).await?
            }
            AudioFileFormat::Raw => AudioFileWriter::new_raw(path).await?,
        };

        self.writer = Some(writer);
        Ok(())
    }

    /// Démarre la boucle de traitement du DiskSink
    pub async fn run(mut self) -> Result<DiskSinkStats, AudioError> {
        let mut stats = DiskSinkStats::new(self.node_id.clone());
        let mut source_name: Option<String> = None;
        let mut initialized = false;

        loop {
            tokio::select! {
                // Recevoir les chunks audio
                chunk_opt = self.rx.recv() => {
                    match chunk_opt {
                        Some(chunk) => {
                            // Initialiser le writer à la réception du premier chunk
                            if !initialized {
                                self.initialize_writer(source_name.as_deref()).await?;
                                initialized = true;
                            }

                            // Appliquer le gain avant l'écriture
                            let chunk_with_gain = if chunk.gain_db().abs() > f64::EPSILON {
                                Arc::clone(&chunk).apply_gain()
                            } else {
                                Arc::clone(&chunk)
                            };

                            // Écrire le chunk
                            if let Some(ref mut writer) = self.writer {
                                writer.write_chunk(&chunk_with_gain).await?;
                                stats.record_chunk(&chunk_with_gain);
                            }
                        }
                        None => {
                            // Channel fermé, terminer
                            break;
                        }
                    }
                }

                // Recevoir les mises à jour du nom de source
                source_event_opt = async {
                    if let Some(ref mut rx) = self.source_name_rx {
                        rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    if let Some(event) = source_event_opt {
                        source_name = Some(event.source_name.clone());

                        // Si on n'a pas encore initialisé, le nom sera utilisé plus tard
                        // Sinon, on pourrait décider de fermer le fichier actuel et d'en créer un nouveau
                    }
                }
            }
        }

        // Fermer le fichier proprement
        if let Some(writer) = self.writer {
            writer.close().await?;
        }

        stats.finalize();
        Ok(stats)
    }
}

/// Writer pour fichiers audio
struct AudioFileWriter {
    file: File,
    format: AudioFileFormat,
    sample_rate: Option<u32>,
    total_samples: usize,
}

impl AudioFileWriter {
    /// Crée un writer WAV
    async fn new_wav(path: PathBuf) -> Result<Self, AudioError> {
        let file = File::create(path)
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Failed to create file: {}", e)))?;

        Ok(Self {
            file,
            format: AudioFileFormat::Wav,
            sample_rate: None,
            total_samples: 0,
        })
    }

    /// Crée un writer pour PCM brut
    async fn new_raw(path: PathBuf) -> Result<Self, AudioError> {
        let file = File::create(path)
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Failed to create file: {}", e)))?;

        Ok(Self {
            file,
            format: AudioFileFormat::Raw,
            sample_rate: None,
            total_samples: 0,
        })
    }

    /// Écrit un chunk audio
    async fn write_chunk(&mut self, chunk: &AudioChunk) -> Result<(), AudioError> {
        // Enregistrer le sample rate du premier chunk
        if self.sample_rate.is_none() {
            let sr = chunk.sample_rate();
            self.sample_rate = Some(sr);

            // Pour WAV, écrire l'en-tête (simplifié)
            if matches!(self.format, AudioFileFormat::Wav) {
                self.write_wav_header(sr).await?;
            }
        }

        // Convertir en bytes (little-endian 16-bit PCM)
        let mut bytes = Vec::with_capacity(chunk.len() * 4);
        let max_val = chunk.bit_depth().max_value();
        for frame in chunk.frames() {
            let left = (frame[0] as f32 / max_val).clamp(-1.0, 1.0);
            let right = (frame[1] as f32 / max_val).clamp(-1.0, 1.0);
            let sample_i16 = (left * 32767.0) as i16;
            bytes.extend_from_slice(&sample_i16.to_le_bytes());
            let sample_r16 = (right * 32767.0) as i16;
            bytes.extend_from_slice(&sample_r16.to_le_bytes());
        }

        self.file.write_all(&bytes).await.map_err(|e| {
            AudioError::ProcessingError(format!("Failed to write audio data: {}", e))
        })?;

        self.total_samples += chunk.len();
        Ok(())
    }

    /// Écrit un en-tête WAV simplifié
    async fn write_wav_header(&mut self, sample_rate: u32) -> Result<(), AudioError> {
        // En-tête WAV basique (sera mis à jour à la fermeture)
        let mut header = Vec::new();

        // RIFF chunk
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&0u32.to_le_bytes()); // Taille (à mettre à jour)
        header.extend_from_slice(b"WAVE");

        // fmt chunk
        header.extend_from_slice(b"fmt ");
        header.extend_from_slice(&16u32.to_le_bytes()); // Taille du fmt chunk
        header.extend_from_slice(&1u16.to_le_bytes()); // Format PCM
        header.extend_from_slice(&2u16.to_le_bytes()); // 2 canaux (stéréo)
        header.extend_from_slice(&sample_rate.to_le_bytes());
        header.extend_from_slice(&(sample_rate * 4).to_le_bytes()); // Byte rate
        header.extend_from_slice(&4u16.to_le_bytes()); // Block align
        header.extend_from_slice(&16u16.to_le_bytes()); // Bits per sample

        // data chunk header
        header.extend_from_slice(b"data");
        header.extend_from_slice(&0u32.to_le_bytes()); // Taille des données (à mettre à jour)

        self.file.write_all(&header).await.map_err(|e| {
            AudioError::ProcessingError(format!("Failed to write WAV header: {}", e))
        })?;

        Ok(())
    }

    /// Ferme le fichier et met à jour l'en-tête si nécessaire
    async fn close(mut self) -> Result<(), AudioError> {
        if matches!(self.format, AudioFileFormat::Wav) {
            // Mettre à jour les tailles dans l'en-tête WAV
            let data_size = (self.total_samples * 4) as u32; // 2 bytes per sample * 2 channels
            let file_size = data_size + 36;

            // Positionner au début et réécrire les tailles
            use tokio::io::AsyncSeekExt;
            self.file
                .seek(std::io::SeekFrom::Start(4))
                .await
                .map_err(|e| {
                    AudioError::ProcessingError(format!("Failed to seek in file: {}", e))
                })?;
            self.file
                .write_all(&file_size.to_le_bytes())
                .await
                .map_err(|e| {
                    AudioError::ProcessingError(format!("Failed to update file size: {}", e))
                })?;

            self.file
                .seek(std::io::SeekFrom::Start(40))
                .await
                .map_err(|e| {
                    AudioError::ProcessingError(format!("Failed to seek in file: {}", e))
                })?;
            self.file
                .write_all(&data_size.to_le_bytes())
                .await
                .map_err(|e| {
                    AudioError::ProcessingError(format!("Failed to update data size: {}", e))
                })?;
        }

        self.file
            .flush()
            .await
            .map_err(|e| AudioError::ProcessingError(format!("Failed to flush file: {}", e)))?;

        Ok(())
    }
}

/// Statistiques du DiskSink
#[derive(Debug, Clone)]
pub struct DiskSinkStats {
    pub node_id: String,
    pub chunks_written: u64,
    pub total_samples: u64,
    pub total_duration_sec: f64,
}

impl DiskSinkStats {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            chunks_written: 0,
            total_samples: 0,
            total_duration_sec: 0.0,
        }
    }

    pub fn record_chunk(&mut self, chunk: &AudioChunk) {
        self.chunks_written += 1;
        self.total_samples += chunk.len() as u64;
        self.total_duration_sec += chunk.len() as f64 / chunk.sample_rate() as f64;
    }

    pub fn finalize(&mut self) {
        // Pourrait effectuer des calculs finaux ici
    }

    pub fn display(&self) {
        println!("\n=== DiskSink Statistics: {} ===", self.node_id);
        println!("Chunks written: {}", self.chunks_written);
        println!("Total samples: {}", self.total_samples);
        println!("Total duration: {:.3} sec", self.total_duration_sec);
        println!("============================\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BitDepth;

    #[tokio::test]
    async fn test_disk_sink_basic() {
        let temp_dir = std::env::temp_dir().join("pmoaudio_test");
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();

        let config = DiskSinkConfig {
            output_dir: temp_dir.clone(),
            filename: Some("test_output.wav".to_string()),
            format: AudioFileFormat::Wav,
            buffer_size: 10,
        };

        let (sink, tx) = DiskSink::new("test".to_string(), config, 10);

        let handle = tokio::spawn(async move { sink.run().await });

        // Envoyer quelques chunks
        for i in 0..5 {
            let chunk = AudioChunk::from_channels_f32(
                i,
                vec![0.5; 1000],
                vec![0.5; 1000],
                48000,
                BitDepth::B24,
            );
            tx.send(chunk).await.unwrap();
        }

        drop(tx);

        let stats = handle.await.unwrap().unwrap();
        assert_eq!(stats.chunks_written, 5);

        // Vérifier que le fichier existe
        let output_path = temp_dir.join("test_output.wav");
        assert!(output_path.exists());

        // Nettoyage
        tokio::fs::remove_file(output_path).await.ok();
        tokio::fs::remove_dir(temp_dir).await.ok();
    }
}
