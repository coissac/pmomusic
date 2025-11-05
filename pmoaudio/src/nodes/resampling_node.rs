//! ResamplingNode - Node de resampling pour normaliser le sample rate
//!
//! Ce node prend en entrée des chunks audio avec des sample rates variables
//! et les resample vers un sample rate cible fixe.
//!
//! # Usage
//!
//! ```rust,no_run
//! use pmoaudio::{ResamplingNode, FileSource};
//!
//! let mut source = FileSource::new("audio.flac");
//! let mut resampler = ResamplingNode::new(48000); // Force 48kHz
//! source.register(Box::new(resampler));
//! ```
//!
//! # Comportement
//!
//! - Détecte automatiquement les changements de sample rate
//! - Recrée le resampler quand nécessaire
//! - Passe les chunks directement si déjà au bon sample rate
//! - Préserve les sync markers (TrackBoundary, etc.)
//!
//! # Performance
//!
//! Le resampling est effectué via libsoxr (très haute qualité).
//! La qualité est adaptée selon la profondeur de bits :
//! - 8-bit : Medium quality
//! - 16-bit : High quality
//! - 24-bit/32-bit : Very high quality

use crate::{
    dsp::resampling::{build_resampler, resampling, Resampler},
    nodes::{AudioError, TypedAudioNode},
    pipeline::{AudioPipelineNode, Node, NodeLogic},
    type_constraints::TypeRequirement,
    AudioChunk, AudioChunkData, AudioSegment, BitDepth, I24,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing;

// ═══════════════════════════════════════════════════════════════════════════
// ResamplingLogic - Logique pure de resampling
// ═══════════════════════════════════════════════════════════════════════════

/// Logique pure de resampling
///
/// Maintient un resampler et le met à jour selon les changements de sample rate.
pub struct ResamplingLogic {
    target_sample_rate: u32,
    current_resampler: Option<ResamplerState>,
}

struct ResamplerState {
    source_hz: u32,
    resampler: Resampler,
}

impl ResamplingLogic {
    pub fn new(target_sample_rate: u32) -> Self {
        Self {
            target_sample_rate,
            current_resampler: None,
        }
    }

    /// Resample un chunk audio vers le sample rate cible
    fn resample_chunk(&mut self, chunk: &AudioChunk) -> Result<AudioChunk, AudioError> {
        let source_sr = chunk.sample_rate();
        let bit_depth = BitDepth::from_audio_chunk(chunk);

        // Si déjà au bon sample rate, retourner tel quel
        if source_sr == self.target_sample_rate {
            return Ok(chunk.clone());
        }

        // Vérifier si on doit recréer le resampler
        let need_new_resampler = match &self.current_resampler {
            None => true,
            Some(state) => state.source_hz != source_sr,
        };

        if need_new_resampler {
            tracing::debug!(
                "ResamplingLogic: creating resampler {}Hz → {}Hz (bit_depth={:?})",
                source_sr,
                self.target_sample_rate,
                bit_depth
            );
            let resampler = build_resampler(source_sr, self.target_sample_rate, bit_depth)
                .map_err(|e| AudioError::ProcessingError(format!("Resampler init failed: {}", e)))?;
            self.current_resampler = Some(ResamplerState {
                source_hz: source_sr,
                resampler,
            });
        }

        let state = self.current_resampler.as_mut().unwrap();

        // Extraire les canaux L/R en i32
        let (left, right) = extract_channels_i32(chunk)?;

        // Appliquer le resampling
        let (resampled_left, resampled_right) = resampling(&left, &right, &mut state.resampler);

        // Recréer le chunk avec le nouveau sample rate
        reconstruct_chunk(chunk, resampled_left, resampled_right, self.target_sample_rate)
    }
}

#[async_trait::async_trait]
impl NodeLogic for ResamplingLogic {
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let mut rx = input.expect("ResamplingNode must have input");
        tracing::debug!(
            "ResamplingLogic::process started, target={}Hz, {} children",
            self.target_sample_rate,
            output.len()
        );

        loop {
            let segment = tokio::select! {
                _ = stop_token.cancelled() => {
                    tracing::debug!("ResamplingLogic cancelled");
                    break;
                }

                result = rx.recv() => {
                    match result {
                        Some(seg) => seg,
                        None => {
                            tracing::debug!("ResamplingLogic received EOF");
                            break;
                        }
                    }
                }
            };

            // Resample si c'est un chunk audio, sinon passer tel quel
            let output_segment = if segment.is_audio_chunk() {
                if let Some(chunk) = segment.as_chunk() {
                    let resampled_chunk = self.resample_chunk(chunk)?;

                    Arc::new(AudioSegment {
                        order: segment.order,
                        timestamp_sec: segment.timestamp_sec,
                        segment: crate::_AudioSegment::Chunk(Arc::new(resampled_chunk)),
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
// Helper Functions
// ═══════════════════════════════════════════════════════════════════════════

/// Extrait les canaux L/R d'un AudioChunk en i32
fn extract_channels_i32(chunk: &AudioChunk) -> Result<(Vec<i32>, Vec<i32>), AudioError> {
    match chunk {
        AudioChunk::I16(data) => {
            let stereo = data.stereo();
            let left = stereo.iter().map(|frame| frame[0] as i32).collect();
            let right = stereo.iter().map(|frame| frame[1] as i32).collect();
            Ok((left, right))
        }
        AudioChunk::I24(data) => {
            let stereo = data.stereo();
            let left = stereo.iter().map(|frame| frame[0].to_i32()).collect();
            let right = stereo.iter().map(|frame| frame[1].to_i32()).collect();
            Ok((left, right))
        }
        AudioChunk::I32(data) => {
            let stereo = data.stereo();
            let left = stereo.iter().map(|frame| frame[0]).collect();
            let right = stereo.iter().map(|frame| frame[1]).collect();
            Ok((left, right))
        }
        AudioChunk::F32(data) => {
            let stereo = data.stereo();
            // Convertir f32 → i32 (dénormaliser)
            let left = stereo
                .iter()
                .map(|frame| (frame[0] * i32::MAX as f32) as i32)
                .collect();
            let right = stereo
                .iter()
                .map(|frame| (frame[1] * i32::MAX as f32) as i32)
                .collect();
            Ok((left, right))
        }
        AudioChunk::F64(data) => {
            let stereo = data.stereo();
            // Convertir f64 → i32 (dénormaliser)
            let left = stereo
                .iter()
                .map(|frame| (frame[0] * i32::MAX as f64) as i32)
                .collect();
            let right = stereo
                .iter()
                .map(|frame| (frame[1] * i32::MAX as f64) as i32)
                .collect();
            Ok((left, right))
        }
    }
}

/// Reconstruit un AudioChunk du même type avec les canaux resamplez
fn reconstruct_chunk(
    original: &AudioChunk,
    left: Vec<i32>,
    right: Vec<i32>,
    new_sample_rate: u32,
) -> Result<AudioChunk, AudioError> {
    if left.len() != right.len() {
        return Err(AudioError::ProcessingError(
            "Left and right channel lengths differ after resampling".into(),
        ));
    }

    let gain_db = original.gain_db();

    match original {
        AudioChunk::I16(_) => {
            let mut stereo = Vec::with_capacity(left.len());
            for i in 0..left.len() {
                stereo.push([left[i] as i16, right[i] as i16]);
            }
            Ok(AudioChunk::I16(AudioChunkData::new(
                stereo,
                new_sample_rate,
                gain_db,
            )))
        }
        AudioChunk::I24(_) => {
            let mut stereo = Vec::with_capacity(left.len());
            for i in 0..left.len() {
                let l = I24::new(left[i])
                    .ok_or_else(|| AudioError::ProcessingError("Invalid I24 value".into()))?;
                let r = I24::new(right[i])
                    .ok_or_else(|| AudioError::ProcessingError("Invalid I24 value".into()))?;
                stereo.push([l, r]);
            }
            Ok(AudioChunk::I24(AudioChunkData::new(
                stereo,
                new_sample_rate,
                gain_db,
            )))
        }
        AudioChunk::I32(_) => {
            let mut stereo = Vec::with_capacity(left.len());
            for i in 0..left.len() {
                stereo.push([left[i], right[i]]);
            }
            Ok(AudioChunk::I32(AudioChunkData::new(
                stereo,
                new_sample_rate,
                gain_db,
            )))
        }
        AudioChunk::F32(_) => {
            let mut stereo = Vec::with_capacity(left.len());
            for i in 0..left.len() {
                stereo.push([
                    left[i] as f32 / i32::MAX as f32,
                    right[i] as f32 / i32::MAX as f32,
                ]);
            }
            Ok(AudioChunk::F32(AudioChunkData::new(
                stereo,
                new_sample_rate,
                gain_db,
            )))
        }
        AudioChunk::F64(_) => {
            let mut stereo = Vec::with_capacity(left.len());
            for i in 0..left.len() {
                stereo.push([
                    left[i] as f64 / i32::MAX as f64,
                    right[i] as f64 / i32::MAX as f64,
                ]);
            }
            Ok(AudioChunk::F64(AudioChunkData::new(
                stereo,
                new_sample_rate,
                gain_db,
            )))
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WRAPPER ResamplingNode - Délègue à Node<ResamplingLogic>
// ═══════════════════════════════════════════════════════════════════════════

/// ResamplingNode - Normalise le sample rate vers une valeur cible
///
/// Ce node prend en entrée des chunks audio avec des sample rates variables
/// et les resample vers un sample rate fixe.
pub struct ResamplingNode {
    inner: Node<ResamplingLogic>,
}

impl ResamplingNode {
    /// Crée un nouveau node de resampling
    ///
    /// * `target_sample_rate` - Sample rate de sortie en Hz (ex: 48000)
    pub fn new(target_sample_rate: u32) -> Box<dyn AudioPipelineNode> {
        Self::with_channel_size(target_sample_rate, 16)
    }

    /// Crée un nouveau node de resampling avec taille de canal personnalisée
    ///
    /// * `target_sample_rate` - Sample rate de sortie en Hz
    /// * `channel_size` - Taille du canal de communication
    pub fn with_channel_size(
        target_sample_rate: u32,
        channel_size: usize,
    ) -> Box<dyn AudioPipelineNode> {
        let logic = ResamplingLogic::new(target_sample_rate);
        Box::new(Self {
            inner: Node::new_with_input(logic, channel_size),
        })
    }
}

#[async_trait::async_trait]
impl AudioPipelineNode for ResamplingNode {
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

impl TypedAudioNode for ResamplingNode {
    fn input_type(&self) -> Option<TypeRequirement> {
        // Accepte n'importe quel type
        Some(TypeRequirement::any())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        // Produit le même type que l'entrée (mais sample rate changé)
        Some(TypeRequirement::any())
    }
}
