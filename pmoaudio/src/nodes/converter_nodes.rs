//! Nodes de conversion de type pour AudioChunk
//!
//! Ces nodes permettent de convertir les chunks audio d'un type vers un autre
//! (I16, I24, I32, F32, F64). Toutes les conversions utilisent les fonctions
//! DSP optimisées SIMD du module `crate::conversions`.
//!
//! Le designer de pipeline doit insérer manuellement ces nodes pour gérer
//! les incompatibilités de type entre producers et consumers.
//!
//! # Nouvelle Architecture
//!
//! Les converters utilisent maintenant `Node<ConverterLogic<F>>` où F est
//! une fonction de conversion. Cela simplifie drastiquement le code (de ~130
//! lignes par converter à ~20 lignes de logique pure).

use crate::{
    nodes::AudioError,
    pipeline::{Node, NodeLogic},
    AudioChunk, AudioPipelineNode, AudioSegment,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Logique de conversion générique
///
/// Cette struct contient la logique pure de conversion d'un type vers un autre.
/// Elle reçoit des segments, convertit les chunks audio, et relay les syncmarkers.
pub struct ConverterLogic<F> {
    convert_fn: F,
}

impl<F> ConverterLogic<F>
where
    F: Fn(&AudioChunk) -> AudioChunk + Send + 'static,
{
    pub fn new(convert_fn: F) -> Self {
        Self { convert_fn }
    }
}

#[async_trait::async_trait]
impl<F> NodeLogic for ConverterLogic<F>
where
    F: Fn(&AudioChunk) -> AudioChunk + Send + 'static,
{
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let mut rx = input.expect("Converter must have input");
        tracing::debug!("ConverterLogic::process started, {} children", output.len());

        loop {
            let segment = tokio::select! {
                _ = stop_token.cancelled() => {
                    tracing::debug!("ConverterLogic cancelled");
                    break;
                }

                result = rx.recv() => {
                    match result {
                        Some(seg) => seg,
                        None => {
                            tracing::debug!("ConverterLogic received EOF");
                            break; // EOF
                        }
                    }
                }
            };

            // Convertir si c'est un chunk audio, sinon passer tel quel
            let output_segment = if segment.is_audio_chunk() {
                if let Some(chunk) = segment.as_chunk() {
                    let converted_chunk = (self.convert_fn)(chunk);

                    // Debug: afficher le type du chunk converti (seulement pour le premier)
                    if segment.order == 0 {
                        let chunk_type = match &converted_chunk {
                            crate::AudioChunk::I16(_) => "I16",
                            crate::AudioChunk::I24(_) => "I24",
                            crate::AudioChunk::I32(_) => "I32",
                            crate::AudioChunk::F32(_) => "F32",
                            crate::AudioChunk::F64(_) => "F64",
                        };
                        tracing::debug!("ConverterLogic: converted chunk type = {}", chunk_type);
                    }

                    Arc::new(AudioSegment {
                        order: segment.order,
                        timestamp_sec: segment.timestamp_sec,
                        segment: crate::_AudioSegment::Chunk(Arc::new(converted_chunk)),
                    })
                } else {
                    segment
                }
            } else {
                segment
            };

            // Envoyer à tous les enfants
            for tx in &output {
                tx.send(output_segment.clone())
                    .await
                    .map_err(|_| AudioError::ChildDied)?;
            }
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Converters spécifiques - Fonctions factory simplifiées
// ═══════════════════════════════════════════════════════════════════════════

/// Node de conversion vers I16 (16-bit signed integer)
pub struct ToI16Node;

impl ToI16Node {
    pub fn new() -> Box<dyn AudioPipelineNode> {
        Self::with_channel_size(16)
    }

    pub fn with_channel_size(channel_size: usize) -> Box<dyn AudioPipelineNode> {
        let logic = ConverterLogic::new(|chunk: &AudioChunk| chunk.to_i16());
        Box::new(Node::new_with_input(logic, channel_size))
    }
}

impl Default for ToI16Node {
    fn default() -> Self {
        Self
    }
}

/// Node de conversion vers I24 (24-bit signed integer)
pub struct ToI24Node;

impl ToI24Node {
    pub fn new() -> Box<dyn AudioPipelineNode> {
        Self::with_channel_size(16)
    }

    pub fn with_channel_size(channel_size: usize) -> Box<dyn AudioPipelineNode> {
        let logic = ConverterLogic::new(|chunk: &AudioChunk| chunk.to_i24());
        Box::new(Node::new_with_input(logic, channel_size))
    }
}

impl Default for ToI24Node {
    fn default() -> Self {
        Self
    }
}

/// Node de conversion vers I32 (32-bit signed integer)
pub struct ToI32Node;

impl ToI32Node {
    pub fn new() -> Box<dyn AudioPipelineNode> {
        Self::with_channel_size(16)
    }

    pub fn with_channel_size(channel_size: usize) -> Box<dyn AudioPipelineNode> {
        let logic = ConverterLogic::new(|chunk: &AudioChunk| chunk.to_i32());
        Box::new(Node::new_with_input(logic, channel_size))
    }
}

impl Default for ToI32Node {
    fn default() -> Self {
        Self
    }
}

/// Node de conversion vers F32 (32-bit floating point)
pub struct ToF32Node;

impl ToF32Node {
    pub fn new() -> Box<dyn AudioPipelineNode> {
        Self::with_channel_size(16)
    }

    pub fn with_channel_size(channel_size: usize) -> Box<dyn AudioPipelineNode> {
        let logic = ConverterLogic::new(|chunk: &AudioChunk| chunk.to_f32());
        Box::new(Node::new_with_input(logic, channel_size))
    }
}

impl Default for ToF32Node {
    fn default() -> Self {
        Self
    }
}

/// Node de conversion vers F64 (64-bit floating point)
pub struct ToF64Node;

impl ToF64Node {
    pub fn new() -> Box<dyn AudioPipelineNode> {
        Self::with_channel_size(16)
    }

    pub fn with_channel_size(channel_size: usize) -> Box<dyn AudioPipelineNode> {
        let logic = ConverterLogic::new(|chunk: &AudioChunk| chunk.to_f64());
        Box::new(Node::new_with_input(logic, channel_size))
    }
}

impl Default for ToF64Node {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioChunk, AudioChunkData};

    #[tokio::test]
    async fn test_converter_logic() {
        // Test unitaire de la logique pure
        let mut logic = ConverterLogic::new(|chunk: &AudioChunk| chunk.to_f32());

        let (input_tx, input_rx) = mpsc::channel(10);
        let (output_tx, mut output_rx) = mpsc::channel(10);
        let stop_token = CancellationToken::new();

        // Créer un chunk de test
        let test_chunk = AudioChunk::I16(AudioChunkData::new(
            vec![[100, 200], [300, 400]],
            48000,
            0.0,
        ));

        // Créer le segment directement
        let segment = Arc::new(AudioSegment {
            order: 0,
            timestamp_sec: 0.0,
            segment: crate::_AudioSegment::Chunk(Arc::new(test_chunk)),
        });

        // Envoyer le segment
        input_tx.send(segment).await.unwrap();
        drop(input_tx); // EOF

        // Lancer le traitement
        tokio::spawn(async move {
            logic
                .process(Some(input_rx), vec![output_tx], stop_token)
                .await
                .unwrap();
        });

        // Vérifier le résultat
        let result = output_rx.recv().await.unwrap();
        assert!(result.is_audio_chunk());

        if let Some(chunk) = result.as_chunk() {
            assert!(matches!(chunk.as_ref(), AudioChunk::F32(_)));
        } else {
            panic!("Expected audio chunk");
        }
    }
}
