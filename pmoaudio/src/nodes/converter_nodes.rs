//! Nodes de conversion de type pour AudioChunk
//!
//! Ces nodes permettent de convertir les chunks audio d'un type vers un autre
//! (I16, I24, I32, F32, F64). Toutes les conversions utilisent les fonctions
//! DSP optimisées SIMD du module `crate::conversions`.
//!
//! Le designer de pipeline doit insérer manuellement ces nodes pour gérer
//! les incompatibilités de type entre producers et consumers.

use crate::{
    nodes::{AudioError, MultiSubscriberNode, TypedAudioNode},
    type_constraints::{SampleType, TypeRequirement},
    AudioSegment,
};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Node de conversion vers I16
///
/// Convertit n'importe quel type de chunk audio vers I16 (16-bit signed integer).
/// Utilise les conversions DSP SIMD optimisées.
pub struct ToI16Node {
    rx: mpsc::Receiver<Arc<AudioSegment>>,
    subscribers: MultiSubscriberNode,
}

impl ToI16Node {
    /// Crée un nouveau node de conversion vers I16
    pub fn new() -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        Self::with_channel_size(16)
    }

    /// Crée un nouveau node avec une taille de buffer spécifique
    pub fn with_channel_size(channel_size: usize) -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        let (tx, rx) = mpsc::channel(channel_size);
        let node = Self {
            rx,
            subscribers: MultiSubscriberNode::new(),
        };
        (node, tx)
    }

    /// Ajoute un abonné qui recevra les segments audio convertis
    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioSegment>>) {
        self.subscribers.add_subscriber(tx);
    }

    /// Lance le traitement de conversion
    pub async fn run(mut self) -> Result<(), AudioError> {
        while let Some(segment) = self.rx.recv().await {
            // Si c'est un syncmarker, passer directement
            if !segment.is_audio_chunk() {
                self.subscribers.push(segment).await?;
                continue;
            }

            // Convertir le chunk audio vers I16
            let converted_segment = if let Some(chunk) = segment.as_chunk() {
                let converted_chunk = chunk.to_i16();
                Arc::new(AudioSegment {
                    order: segment.order,
                    timestamp_sec: segment.timestamp_sec,
                    segment: crate::_AudioSegment::Chunk(Arc::new(converted_chunk)),
                })
            } else {
                segment
            };

            self.subscribers.push(converted_segment).await?;
        }

        Ok(())
    }
}

impl TypedAudioNode for ToI16Node {
    fn input_type(&self) -> Option<TypeRequirement> {
        // Accepte n'importe quel type
        Some(TypeRequirement::any())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        // Produit uniquement I16
        Some(TypeRequirement::specific(SampleType::I16))
    }
}

impl Default for ToI16Node {
    fn default() -> Self {
        Self::new().0
    }
}

/// Node de conversion vers I24
///
/// Convertit n'importe quel type de chunk audio vers I24 (24-bit signed integer).
/// Utilise les conversions DSP SIMD optimisées.
pub struct ToI24Node {
    rx: mpsc::Receiver<Arc<AudioSegment>>,
    subscribers: MultiSubscriberNode,
}

impl ToI24Node {
    /// Crée un nouveau node de conversion vers I24
    pub fn new() -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        Self::with_channel_size(16)
    }

    /// Crée un nouveau node avec une taille de buffer spécifique
    pub fn with_channel_size(channel_size: usize) -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        let (tx, rx) = mpsc::channel(channel_size);
        let node = Self {
            rx,
            subscribers: MultiSubscriberNode::new(),
        };
        (node, tx)
    }

    /// Ajoute un abonné qui recevra les segments audio convertis
    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioSegment>>) {
        self.subscribers.add_subscriber(tx);
    }

    /// Lance le traitement de conversion
    pub async fn run(mut self) -> Result<(), AudioError> {
        while let Some(segment) = self.rx.recv().await {
            if !segment.is_audio_chunk() {
                self.subscribers.push(segment).await?;
                continue;
            }

            let converted_segment = if let Some(chunk) = segment.as_chunk() {
                let converted_chunk = chunk.to_i24();
                Arc::new(AudioSegment {
                    order: segment.order,
                    timestamp_sec: segment.timestamp_sec,
                    segment: crate::_AudioSegment::Chunk(Arc::new(converted_chunk)),
                })
            } else {
                segment
            };

            self.subscribers.push(converted_segment).await?;
        }

        Ok(())
    }
}

impl TypedAudioNode for ToI24Node {
    fn input_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::any())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::specific(SampleType::I24))
    }
}

impl Default for ToI24Node {
    fn default() -> Self {
        Self::new().0
    }
}

/// Node de conversion vers I32
///
/// Convertit n'importe quel type de chunk audio vers I32 (32-bit signed integer).
/// Utilise les conversions DSP SIMD optimisées.
pub struct ToI32Node {
    rx: mpsc::Receiver<Arc<AudioSegment>>,
    subscribers: MultiSubscriberNode,
}

impl ToI32Node {
    /// Crée un nouveau node de conversion vers I32
    pub fn new() -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        Self::with_channel_size(16)
    }

    /// Crée un nouveau node avec une taille de buffer spécifique
    pub fn with_channel_size(channel_size: usize) -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        let (tx, rx) = mpsc::channel(channel_size);
        let node = Self {
            rx,
            subscribers: MultiSubscriberNode::new(),
        };
        (node, tx)
    }

    /// Ajoute un abonné qui recevra les segments audio convertis
    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioSegment>>) {
        self.subscribers.add_subscriber(tx);
    }

    /// Lance le traitement de conversion
    pub async fn run(mut self) -> Result<(), AudioError> {
        while let Some(segment) = self.rx.recv().await {
            if !segment.is_audio_chunk() {
                self.subscribers.push(segment).await?;
                continue;
            }

            let converted_segment = if let Some(chunk) = segment.as_chunk() {
                let converted_chunk = chunk.to_i32();
                Arc::new(AudioSegment {
                    order: segment.order,
                    timestamp_sec: segment.timestamp_sec,
                    segment: crate::_AudioSegment::Chunk(Arc::new(converted_chunk)),
                })
            } else {
                segment
            };

            self.subscribers.push(converted_segment).await?;
        }

        Ok(())
    }
}

impl TypedAudioNode for ToI32Node {
    fn input_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::any())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::specific(SampleType::I32))
    }
}

impl Default for ToI32Node {
    fn default() -> Self {
        Self::new().0
    }
}

/// Node de conversion vers F32
///
/// Convertit n'importe quel type de chunk audio vers F32 (32-bit floating point).
/// Utilise les conversions DSP SIMD optimisées.
pub struct ToF32Node {
    rx: mpsc::Receiver<Arc<AudioSegment>>,
    subscribers: MultiSubscriberNode,
}

impl ToF32Node {
    /// Crée un nouveau node de conversion vers F32
    pub fn new() -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        Self::with_channel_size(16)
    }

    /// Crée un nouveau node avec une taille de buffer spécifique
    pub fn with_channel_size(channel_size: usize) -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        let (tx, rx) = mpsc::channel(channel_size);
        let node = Self {
            rx,
            subscribers: MultiSubscriberNode::new(),
        };
        (node, tx)
    }

    /// Ajoute un abonné qui recevra les segments audio convertis
    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioSegment>>) {
        self.subscribers.add_subscriber(tx);
    }

    /// Lance le traitement de conversion
    pub async fn run(mut self) -> Result<(), AudioError> {
        while let Some(segment) = self.rx.recv().await {
            if !segment.is_audio_chunk() {
                self.subscribers.push(segment).await?;
                continue;
            }

            let converted_segment = if let Some(chunk) = segment.as_chunk() {
                let converted_chunk = chunk.to_f32();
                Arc::new(AudioSegment {
                    order: segment.order,
                    timestamp_sec: segment.timestamp_sec,
                    segment: crate::_AudioSegment::Chunk(Arc::new(converted_chunk)),
                })
            } else {
                segment
            };

            self.subscribers.push(converted_segment).await?;
        }

        Ok(())
    }
}

impl TypedAudioNode for ToF32Node {
    fn input_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::any())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::specific(SampleType::F32))
    }
}

impl Default for ToF32Node {
    fn default() -> Self {
        Self::new().0
    }
}

/// Node de conversion vers F64
///
/// Convertit n'importe quel type de chunk audio vers F64 (64-bit floating point).
/// Utilise les conversions DSP SIMD optimisées.
pub struct ToF64Node {
    rx: mpsc::Receiver<Arc<AudioSegment>>,
    subscribers: MultiSubscriberNode,
}

impl ToF64Node {
    /// Crée un nouveau node de conversion vers F64
    pub fn new() -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        Self::with_channel_size(16)
    }

    /// Crée un nouveau node avec une taille de buffer spécifique
    pub fn with_channel_size(channel_size: usize) -> (Self, mpsc::Sender<Arc<AudioSegment>>) {
        let (tx, rx) = mpsc::channel(channel_size);
        let node = Self {
            rx,
            subscribers: MultiSubscriberNode::new(),
        };
        (node, tx)
    }

    /// Ajoute un abonné qui recevra les segments audio convertis
    pub fn add_subscriber(&mut self, tx: mpsc::Sender<Arc<AudioSegment>>) {
        self.subscribers.add_subscriber(tx);
    }

    /// Lance le traitement de conversion
    pub async fn run(mut self) -> Result<(), AudioError> {
        while let Some(segment) = self.rx.recv().await {
            if !segment.is_audio_chunk() {
                self.subscribers.push(segment).await?;
                continue;
            }

            let converted_segment = if let Some(chunk) = segment.as_chunk() {
                let converted_chunk = chunk.to_f64();
                Arc::new(AudioSegment {
                    order: segment.order,
                    timestamp_sec: segment.timestamp_sec,
                    segment: crate::_AudioSegment::Chunk(Arc::new(converted_chunk)),
                })
            } else {
                segment
            };

            self.subscribers.push(converted_segment).await?;
        }

        Ok(())
    }
}

impl TypedAudioNode for ToF64Node {
    fn input_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::any())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        Some(TypeRequirement::specific(SampleType::F64))
    }
}

impl Default for ToF64Node {
    fn default() -> Self {
        Self::new().0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioChunk, AudioChunkData};

    #[tokio::test]
    async fn test_to_f32_node_type_requirements() {
        let (node, _tx) = ToF32Node::new();

        // Vérifier les types d'entrée/sortie
        assert_eq!(
            node.input_type().unwrap().get_accepted_types().len(),
            5,
            "Should accept all 5 types"
        );
        assert_eq!(
            node.output_type()
                .unwrap()
                .get_accepted_types()
                .first()
                .copied(),
            Some(SampleType::F32),
            "Should output F32 only"
        );
    }

    #[tokio::test]
    async fn test_to_i16_node_converts_from_i32() {
        let (mut node, tx) = ToI16Node::new();
        let (out_tx, mut out_rx) = mpsc::channel(16);
        node.add_subscriber(out_tx);

        // Lancer le node dans une tâche
        let handle = tokio::spawn(async move { node.run().await });

        // Créer et envoyer un chunk I32
        let stereo = vec![[1_000_000i32 << 16, -500_000i32 << 16]; 100];
        let chunk_data = AudioChunkData::new(stereo.clone(), 48_000, 0.0);
        let chunk = AudioChunk::I32(chunk_data);
        let segment = Arc::new(AudioSegment {
            order: 0,
            timestamp_sec: 0.0,
            segment: crate::_AudioSegment::Chunk(Arc::new(chunk)),
        });

        tx.send(segment).await.unwrap();
        drop(tx);

        // Recevoir le chunk converti
        let result = out_rx.recv().await.unwrap();
        assert!(result.is_audio_chunk());

        if let Some(converted) = result.as_chunk() {
            assert_eq!(converted.type_name(), "i16");
            assert_eq!(converted.len(), 100);

            // Vérifier la conversion (downsampling de I32 vers I16)
            if let AudioChunk::I16(data) = &**converted {
                for (orig, converted_frame) in stereo.iter().zip(data.frames().iter()) {
                    let expected_l = (orig[0] >> 16) as i16;
                    let expected_r = (orig[1] >> 16) as i16;
                    assert_eq!(converted_frame[0], expected_l);
                    assert_eq!(converted_frame[1], expected_r);
                }
            }
        }

        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_syncmarkers_passthrough() {
        let (mut node, tx) = ToI16Node::new();
        let (out_tx, mut out_rx) = mpsc::channel(16);
        node.add_subscriber(out_tx);

        tokio::spawn(async move {
            node.run().await.unwrap();
        });

        // Envoyer un syncmarker
        let top_zero = AudioSegment::new_top_zero_sync();
        tx.send(top_zero.clone()).await.unwrap();
        drop(tx);

        // Recevoir le syncmarker
        let result = out_rx.recv().await.unwrap();
        assert!(!result.is_audio_chunk());
        assert!(result.as_sync_marker().is_some());
    }
}
