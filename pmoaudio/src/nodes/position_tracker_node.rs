//! PositionTrackerNode — nœud transparent de suivi de position.
//!
//! Laisse passer tous les segments sans modification, et maintient
//! un compteur de position courant basé sur le `timestamp_sec` des chunks.
//!
//! Placé après un `TimerBufferNode`, il reflète la position de l'audio
//! effectivement sorti du buffer, pas celui encore en attente.

use crate::{
    nodes::AudioError,
    pipeline::{send_to_children, AudioPipelineNode, Node, NodeLogic},
    type_constraints::TypeRequirement,
    AudioSegment, _AudioSegment,
};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// ─── Handle public ────────────────────────────────────────────────────────────

/// Handle partageable pour lire la position courante.
#[derive(Clone)]
pub struct PositionHandle {
    /// Position encodée en microsecondes dans un AtomicU64 (pas de lock nécessaire).
    position_us: Arc<AtomicU64>,
}

impl PositionHandle {
    /// Retourne la position courante en secondes.
    pub fn current_position_sec(&self) -> f64 {
        self.position_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }
}

// ─── Logique du nœud ─────────────────────────────────────────────────────────

struct PositionTrackerLogic {
    position_us: Arc<AtomicU64>,
}

#[async_trait::async_trait]
impl NodeLogic for PositionTrackerLogic {
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let mut input = input.ok_or_else(|| {
            AudioError::ProcessingError("PositionTrackerNode requires an input".into())
        })?;

        loop {
            tokio::select! {
                _ = stop_token.cancelled() => break,
                segment = input.recv() => {
                    match segment {
                        None => break,
                        Some(seg) => {
                            // Mettre à jour la position sur les chunks audio uniquement
                            if let _AudioSegment::Chunk(_) = &seg.segment {
                                let us = (seg.timestamp_sec * 1_000_000.0) as u64;
                                self.position_us.store(us, Ordering::Relaxed);
                            }
                            // Passer le segment sans modification
                            send_to_children("PositionTrackerNode", &output, seg).await?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn cleanup(
        &mut self,
        _reason: crate::pipeline::StopReason,
    ) -> Result<(), AudioError> {
        Ok(())
    }
}

// ─── Nœud public ─────────────────────────────────────────────────────────────

pub struct PositionTrackerNode {
    inner: Node<PositionTrackerLogic>,
}

impl PositionTrackerNode {
    pub fn new() -> (Self, PositionHandle) {
        let position_us = Arc::new(AtomicU64::new(0));
        let logic = PositionTrackerLogic { position_us: position_us.clone() };
        let handle = PositionHandle { position_us };
        let node = Self { inner: Node::new_with_input(logic, 16) };
        (node, handle)
    }
}

#[async_trait::async_trait]
impl AudioPipelineNode for PositionTrackerNode {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, child: Box<dyn AudioPipelineNode>) {
        self.inner.register(child);
    }

    async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }

    fn start(self: Box<Self>) -> crate::pipeline::PipelineHandle {
        Box::new(self.inner).start()
    }
}

impl crate::TypedAudioNode for PositionTrackerNode {
    fn input_type(&self) -> Option<TypeRequirement> {
        None // Accepte tout
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        None // Passe tout
    }
}
