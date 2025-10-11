//! MpdSink - Envoie le flux audio à un démon MPD (Music Player Daemon)
//!
//! Ce module fournit un sink qui streame l'audio vers un démon MPD distant ou local.
//! Note: Cette implémentation est une version mock/skeleton. Une vraie implémentation
//! nécessiterait le protocole MPD complet et l'utilisation de bibliothèques comme `mpd`.

use crate::{nodes::AudioError, AudioChunk};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Configuration pour le MpdSink
#[derive(Debug, Clone)]
pub struct MpdConfig {
    /// Adresse du serveur MPD
    pub host: String,

    /// Port du serveur MPD (défaut: 6600)
    pub port: u16,

    /// Mot de passe optionnel
    pub password: Option<String>,

    /// Nom de l'output MPD à utiliser (optionnel)
    pub output_name: Option<String>,

    /// Taille du buffer
    pub buffer_size: usize,

    /// Format d'envoi
    pub format: MpdAudioFormat,
}

impl Default for MpdConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 6600,
            password: None,
            output_name: None,
            buffer_size: 50,
            format: MpdAudioFormat::S16Le,
        }
    }
}

/// Formats audio supportés par MPD
#[derive(Debug, Clone, Copy)]
pub enum MpdAudioFormat {
    /// Signed 16-bit Little Endian
    S16Le,
    /// Signed 24-bit Little Endian
    S24Le,
    /// Signed 32-bit Little Endian
    S32Le,
    /// Float 32-bit
    F32,
}

impl MpdAudioFormat {
    /// Retourne le nom du format pour le protocole MPD
    pub fn as_mpd_string(&self) -> &str {
        match self {
            MpdAudioFormat::S16Le => "16:16:2",
            MpdAudioFormat::S24Le => "24:24:2",
            MpdAudioFormat::S32Le => "32:32:2",
            MpdAudioFormat::F32 => "f:32:2",
        }
    }
}

/// MpdSink - Streame vers un démon MPD
///
/// Ce sink se connecte à un serveur MPD et lui envoie le flux audio.
/// MPD peut ensuite router l'audio vers différents outputs (ALSA, PulseAudio, HTTP, etc.).
///
/// # Implémentation actuelle
///
/// Cette version est un mock qui simule la communication avec MPD.
/// Pour une vraie implémentation, il faudrait:
/// - Implémenter le protocole MPD (commandes textuelles sur TCP)
/// - S'authentifier si nécessaire
/// - Configurer le format audio
/// - Envoyer les données PCM via le protocole approprié
/// - Gérer les commandes de contrôle (play, pause, stop)
///
/// # Exemples
///
/// ```no_run
/// use pmoaudio::{MpdSink, MpdConfig};
///
/// #[tokio::main]
/// async fn main() {
///     let config = MpdConfig {
///         host: "localhost".to_string(),
///         port: 6600,
///         password: None,
///         ..Default::default()
///     };
///
///     let (sink, sink_tx) = MpdSink::new("mpd1".to_string(), config, 10);
///
///     tokio::spawn(async move {
///         sink.run().await.unwrap()
///     });
/// }
/// ```
pub struct MpdSink {
    /// Identifiant du sink
    node_id: String,

    /// Channel pour recevoir les chunks audio
    rx: mpsc::Receiver<Arc<AudioChunk>>,

    /// Configuration
    config: MpdConfig,

    /// État de la connexion (mock)
    connected: bool,

    /// Version du serveur MPD (mock)
    mpd_version: Option<String>,
}

impl MpdSink {
    /// Crée un nouveau MpdSink
    ///
    /// # Arguments
    ///
    /// * `node_id` - Identifiant unique du sink
    /// * `config` - Configuration MPD
    /// * `channel_size` - Taille du buffer du channel
    pub fn new(
        node_id: String,
        config: MpdConfig,
        channel_size: usize,
    ) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (tx, rx) = mpsc::channel(channel_size);

        let sink = Self {
            node_id,
            rx,
            config,
            connected: false,
            mpd_version: None,
        };

        (sink, tx)
    }

    /// Établit la connexion avec le serveur MPD (mock)
    async fn connect(&mut self) -> Result<(), AudioError> {
        println!(
            "[{}] Connecting to MPD at {}:{}...",
            self.node_id, self.config.host, self.config.port
        );

        // Simuler une connexion TCP
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Dans une vraie implémentation:
        // 1. Établir connexion TCP
        // 2. Lire la bannière de version
        // 3. S'authentifier si password fourni
        // 4. Configurer le format audio

        self.mpd_version = Some("0.23.0".to_string());
        self.connected = true;

        println!(
            "[{}] Connected to MPD v{} successfully",
            self.node_id,
            self.mpd_version.as_ref().unwrap()
        );

        // Configurer le format audio
        self.configure_audio_format().await?;

        Ok(())
    }

    /// Configure le format audio sur MPD (mock)
    async fn configure_audio_format(&self) -> Result<(), AudioError> {
        println!(
            "[{}] Configuring audio format: {}",
            self.node_id,
            self.config.format.as_mpd_string()
        );

        // Dans une vraie implémentation:
        // Envoyer une commande MPD pour configurer le format

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Ok(())
    }

    /// Envoie un chunk au serveur MPD (mock)
    async fn send_chunk(&self, _chunk: &AudioChunk) -> Result<(), AudioError> {
        if !self.connected {
            return Err(AudioError::ProcessingError("Not connected to MPD".to_string()));
        }

        // Dans une vraie implémentation:
        // 1. Appliquer le gain
        // 2. Convertir dans le format approprié (S16LE, etc.)
        // 3. Envoyer via le protocole MPD (probablement via une commande `sendmessage` ou pipe)

        // Simuler un délai d'envoi
        tokio::time::sleep(tokio::time::Duration::from_micros(50)).await;

        Ok(())
    }

    /// Déconnecte proprement du serveur MPD (mock)
    async fn disconnect(&mut self) -> Result<(), AudioError> {
        if self.connected {
            println!("[{}] Disconnecting from MPD...", self.node_id);

            // Dans une vraie implémentation:
            // Envoyer la commande "close"
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            self.connected = false;

            println!("[{}] Disconnected successfully", self.node_id);
        }

        Ok(())
    }

    /// Démarre la boucle de traitement du MpdSink
    pub async fn run(mut self) -> Result<MpdStats, AudioError> {
        // Établir la connexion
        self.connect().await?;

        let mut stats = MpdStats::new(
            self.node_id.clone(),
            format!("{}:{}", self.config.host, self.config.port),
        );

        // Boucle principale
        while let Some(chunk) = self.rx.recv().await {
            // Appliquer le gain si nécessaire
            let chunk_to_send = if (chunk.gain - 1.0).abs() > f32::EPSILON {
                chunk.apply_gain()
            } else {
                (*chunk).clone()
            };

            // Envoyer au serveur MPD
            self.send_chunk(&chunk_to_send).await?;

            stats.record_chunk(&chunk_to_send);
        }

        // Déconnexion propre
        self.disconnect().await?;

        stats.finalize();
        Ok(stats)
    }

    /// Retourne un handle pour contrôler le sink (mock)
    pub fn get_handle(&self) -> MpdHandle {
        MpdHandle {
            node_id: self.node_id.clone(),
        }
    }
}

/// Handle pour contrôler le MpdSink
///
/// Permet d'envoyer des commandes de contrôle au serveur MPD
#[derive(Clone)]
pub struct MpdHandle {
    node_id: String,
}

impl MpdHandle {
    /// Commande play (mock)
    pub async fn play(&self) -> Result<(), AudioError> {
        println!("[{}] MPD command: play", self.node_id);
        Ok(())
    }

    /// Commande pause (mock)
    pub async fn pause(&self) -> Result<(), AudioError> {
        println!("[{}] MPD command: pause", self.node_id);
        Ok(())
    }

    /// Commande stop (mock)
    pub async fn stop(&self) -> Result<(), AudioError> {
        println!("[{}] MPD command: stop", self.node_id);
        Ok(())
    }

    /// Change le volume MPD (0-100) (mock)
    pub async fn set_volume(&self, volume: u8) -> Result<(), AudioError> {
        let clamped = volume.min(100);
        println!("[{}] MPD command: setvol {}", self.node_id, clamped);
        Ok(())
    }
}

/// Statistiques du MpdSink
#[derive(Debug, Clone)]
pub struct MpdStats {
    pub node_id: String,
    pub server_address: String,
    pub chunks_sent: u64,
    pub total_samples: u64,
    pub total_duration_sec: f64,
}

impl MpdStats {
    pub fn new(node_id: String, server_address: String) -> Self {
        Self {
            node_id,
            server_address,
            chunks_sent: 0,
            total_samples: 0,
            total_duration_sec: 0.0,
        }
    }

    pub fn record_chunk(&mut self, chunk: &AudioChunk) {
        self.chunks_sent += 1;
        self.total_samples += chunk.len() as u64;
        self.total_duration_sec += chunk.len() as f64 / chunk.sample_rate as f64;
    }

    pub fn finalize(&mut self) {
        // Calculs finaux si nécessaire
    }

    pub fn display(&self) {
        println!("\n=== MPD Sink Statistics: {} ===", self.node_id);
        println!("Server: {}", self.server_address);
        println!("Chunks sent: {}", self.chunks_sent);
        println!("Total samples: {}", self.total_samples);
        println!("Total duration: {:.3} sec", self.total_duration_sec);
        println!("===============================\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mpd_sink_basic() {
        let config = MpdConfig {
            host: "localhost".to_string(),
            port: 6600,
            ..Default::default()
        };

        let (sink, tx) = MpdSink::new("test".to_string(), config, 10);

        let handle = tokio::spawn(async move { sink.run().await });

        // Envoyer quelques chunks
        for i in 0..5 {
            let chunk = AudioChunk::new(i, vec![0.5; 1000], vec![0.5; 1000], 48000);
            tx.send(Arc::new(chunk)).await.unwrap();
        }

        drop(tx);

        let stats = handle.await.unwrap().unwrap();
        assert_eq!(stats.chunks_sent, 5);
        assert_eq!(stats.server_address, "localhost:6600");
    }

    #[tokio::test]
    async fn test_mpd_handle() {
        let config = MpdConfig::default();
        let (sink, _tx) = MpdSink::new("test".to_string(), config, 10);

        let handle = sink.get_handle();

        // Tester les commandes (mock)
        handle.play().await.unwrap();
        handle.pause().await.unwrap();
        handle.set_volume(75).await.unwrap();
        handle.stop().await.unwrap();
    }
}
