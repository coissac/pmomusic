//! ChromecastSink - Diffuse le flux audio vers un périphérique Chromecast
//!
//! Ce module fournit un sink qui envoie le flux audio à un Chromecast.
//! Note: Cette implémentation est une version mock/skeleton. Une vraie implémentation
//! nécessiterait une bibliothèque comme `rust-cast` ou similaire.

use crate::{nodes::AudioError, AudioChunk};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Configuration pour le ChromecastSink
#[derive(Debug, Clone)]
pub struct ChromecastConfig {
    /// Nom ou adresse IP du Chromecast
    pub device_address: String,

    /// Nom amical du device
    pub device_name: String,

    /// Port de communication (défaut: 8009)
    pub port: u16,

    /// Taille du buffer de streaming
    pub buffer_size: usize,

    /// Format d'encodage pour le streaming
    pub encoding: StreamEncoding,
}

impl Default for ChromecastConfig {
    fn default() -> Self {
        Self {
            device_address: "192.168.1.100".to_string(),
            device_name: "Living Room".to_string(),
            port: 8009,
            buffer_size: 50,
            encoding: StreamEncoding::Mp3,
        }
    }
}

/// Formats d'encodage supportés pour le streaming
#[derive(Debug, Clone, Copy)]
pub enum StreamEncoding {
    /// MP3 (compatible avec la plupart des Chromecasts)
    Mp3,
    /// AAC
    Aac,
    /// Opus
    Opus,
    /// PCM non compressé (haute qualité, bande passante élevée)
    Pcm,
}

/// ChromecastSink - Diffuse vers un périphérique Chromecast
///
/// Ce sink encode le flux audio et le streame vers un Chromecast.
/// La connexion est établie lors de l'initialisation et maintenue pendant toute la durée.
///
/// # Implémentation actuelle
///
/// Cette version est un mock qui simule l'envoi au Chromecast.
/// Pour une vraie implémentation, il faudrait:
/// - Utiliser une bibliothèque comme `rust-cast`
/// - Établir une connexion TLS avec le device
/// - Lancer une application de récepteur sur le Chromecast
/// - Encoder l'audio dans le format approprié
/// - Streamer via HTTP ou WebSocket
///
/// # Exemples
///
/// ```no_run
/// use pmoaudio::{ChromecastSink, ChromecastConfig};
///
/// #[tokio::main]
/// async fn main() {
///     let config = ChromecastConfig {
///         device_address: "192.168.1.100".to_string(),
///         device_name: "Living Room".to_string(),
///         ..Default::default()
///     };
///
///     let (sink, sink_tx) = ChromecastSink::new("chromecast1".to_string(), config, 10);
///
///     tokio::spawn(async move {
///         sink.run().await.unwrap()
///     });
/// }
/// ```
pub struct ChromecastSink {
    /// Identifiant du sink
    node_id: String,

    /// Channel pour recevoir les chunks audio
    rx: mpsc::Receiver<Arc<AudioChunk>>,

    /// Configuration
    config: ChromecastConfig,

    /// État de la connexion (mock)
    connected: bool,
}

impl ChromecastSink {
    /// Crée un nouveau ChromecastSink
    ///
    /// # Arguments
    ///
    /// * `node_id` - Identifiant unique du sink
    /// * `config` - Configuration du Chromecast
    /// * `channel_size` - Taille du buffer du channel
    pub fn new(
        node_id: String,
        config: ChromecastConfig,
        channel_size: usize,
    ) -> (Self, mpsc::Sender<Arc<AudioChunk>>) {
        let (tx, rx) = mpsc::channel(channel_size);

        let sink = Self {
            node_id,
            rx,
            config,
            connected: false,
        };

        (sink, tx)
    }

    /// Établit la connexion avec le Chromecast (mock)
    async fn connect(&mut self) -> Result<(), AudioError> {
        println!(
            "[{}] Connecting to Chromecast '{}' at {}:{}...",
            self.node_id, self.config.device_name, self.config.device_address, self.config.port
        );

        // Simuler une connexion
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        self.connected = true;

        println!(
            "[{}] Connected to Chromecast '{}' successfully",
            self.node_id, self.config.device_name
        );

        Ok(())
    }

    /// Envoie un chunk au Chromecast (mock)
    async fn send_chunk(&self, _chunk: &AudioChunk) -> Result<(), AudioError> {
        if !self.connected {
            return Err(AudioError::ProcessingError(
                "Not connected to Chromecast".to_string(),
            ));
        }

        // Dans une vraie implémentation:
        // 1. Appliquer le gain
        // 2. Encoder dans le format approprié (MP3, AAC, etc.)
        // 3. Envoyer via le protocole Chromecast

        // Pour l'instant, simplement simuler un délai d'envoi
        tokio::time::sleep(tokio::time::Duration::from_micros(50)).await;

        Ok(())
    }

    /// Déconnecte proprement du Chromecast (mock)
    async fn disconnect(&mut self) -> Result<(), AudioError> {
        if self.connected {
            println!(
                "[{}] Disconnecting from Chromecast '{}'...",
                self.node_id, self.config.device_name
            );

            // Simuler la déconnexion
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

            self.connected = false;

            println!("[{}] Disconnected successfully", self.node_id);
        }

        Ok(())
    }

    /// Démarre la boucle de traitement du ChromecastSink
    pub async fn run(mut self) -> Result<ChromecastStats, AudioError> {
        // Établir la connexion
        self.connect().await?;

        let mut stats = ChromecastStats::new(
            self.node_id.clone(),
            self.config.device_name.clone(),
        );

        // Boucle principale
        while let Some(chunk) = self.rx.recv().await {
            // Appliquer le gain si nécessaire
            let chunk_to_send = if (chunk.gain - 1.0).abs() > f32::EPSILON {
                chunk.apply_gain()
            } else {
                (*chunk).clone()
            };

            // Envoyer au Chromecast
            self.send_chunk(&chunk_to_send).await?;

            stats.record_chunk(&chunk_to_send);
        }

        // Déconnexion propre
        self.disconnect().await?;

        stats.finalize();
        Ok(stats)
    }
}

/// Statistiques du ChromecastSink
#[derive(Debug, Clone)]
pub struct ChromecastStats {
    pub node_id: String,
    pub device_name: String,
    pub chunks_sent: u64,
    pub total_samples: u64,
    pub total_duration_sec: f64,
}

impl ChromecastStats {
    pub fn new(node_id: String, device_name: String) -> Self {
        Self {
            node_id,
            device_name,
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
        println!("\n=== Chromecast Statistics: {} ===", self.node_id);
        println!("Device: {}", self.device_name);
        println!("Chunks sent: {}", self.chunks_sent);
        println!("Total samples: {}", self.total_samples);
        println!("Total duration: {:.3} sec", self.total_duration_sec);
        println!("==================================\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chromecast_sink_basic() {
        let config = ChromecastConfig {
            device_address: "127.0.0.1".to_string(),
            device_name: "Test Device".to_string(),
            ..Default::default()
        };

        let (sink, tx) = ChromecastSink::new("test".to_string(), config, 10);

        let handle = tokio::spawn(async move { sink.run().await });

        // Envoyer quelques chunks
        for i in 0..5 {
            let chunk = AudioChunk::new(i, vec![0.5; 1000], vec![0.5; 1000], 48000);
            tx.send(Arc::new(chunk)).await.unwrap();
        }

        drop(tx);

        let stats = handle.await.unwrap().unwrap();
        assert_eq!(stats.chunks_sent, 5);
        assert_eq!(stats.device_name, "Test Device");
    }
}
