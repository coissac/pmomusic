//! TimerNode - Régule le débit des chunks audio en fonction de leurs timestamps
//!
//! Ce node implémente un pacing temporel pour éviter que les sources rapides
//! saturent les sinks lents. Il tolère une avance configurable (buffer) et
//! attend activement pour maintenir la synchronisation temps réel.
//!
//! # Use Cases
//!
//! - **Progressive caching**: Empêche PlaylistSource de lire plus vite que FlacCacheSink n'écrit
//! - **Rate limiting**: Contrôle le débit de n'importe quel pipeline audio
//! - **Streaming**: Synchronise la production avec la consommation temps réel
//!
//! # Exemple
//!
//! ```no_run
//! use pmoaudio::{PlaylistSource, TimerNode, FlacCacheSink};
//!
//! let mut source = PlaylistSource::new(reader, cache);
//! let mut timer = TimerNode::new(3.0); // 3s d'avance max
//! let mut sink = FlacCacheSink::new(cache, covers);
//!
//! source.register(Box::new(timer));
//! timer.register(Box::new(sink));
//! ```
//!
//! # Architecture
//!
//! ```text
//! PlaylistSource → TimerNode → FlacCacheSink
//!      ↓              ↓              ↓
//!  Lit à fond    Régule en    Écrit au
//!                temps réel   bon rythme
//! ```
//!
//! Le TimerNode:
//! 1. Reçoit des chunks avec timestamps
//! 2. Compare `chunk.timestamp_sec` avec le temps écoulé depuis `TopZeroSync`
//! 3. Si l'avance > `max_lead_time_sec`, attend: `sleep(avance - max_lead_time)`
//! 4. Transmet le chunk aux enfants
//!
//! # Markers Supportés
//!
//! - **TopZeroSync**: Reset le timer de référence (instant zero)
//! - **TrackBoundary**: Passthrough transparent
//! - **Heartbeat**: Passthrough transparent
//! - **EndOfStream**: Passthrough transparent
//!
//! # Performance
//!
//! - **CPU**: Quasi-nul (tokio::time::sleep efficace)
//! - **Latency**: Ajoute `max_lead_time_sec` de buffering
//! - **Memory**: Minimal (pas de buffer de chunks)

use crate::{
    nodes::{AudioError, TypedAudioNode, DEFAULT_CHANNEL_SIZE},
    pipeline::{AudioPipelineNode, Node, NodeLogic},
    type_constraints::TypeRequirement,
    AudioSegment, SyncMarker, _AudioSegment,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

// ═══════════════════════════════════════════════════════════════════════════
// TimerNodeLogic - Logique pure de pacing temporel
// ═══════════════════════════════════════════════════════════════════════════

/// Logique pure de régulation temporelle
///
/// Contrôle le débit des chunks audio pour éviter qu'une source rapide
/// sature un sink lent (ex: progressive caching).
pub struct TimerNodeLogic {
    /// Avance maximale tolérée en secondes (buffer)
    max_lead_time_sec: f64,
    /// Instant de référence (reset au TopZeroSync)
    start_time: Option<Instant>,
}

impl TimerNodeLogic {
    pub fn new(max_lead_time_sec: f64) -> Self {
        Self {
            max_lead_time_sec: max_lead_time_sec.max(0.0),
            start_time: None,
        }
    }
}

#[async_trait::async_trait]
impl NodeLogic for TimerNodeLogic {
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let mut rx = input.expect("TimerNode must have input");
        tracing::info!(
            "TimerNodeLogic::process started (max_lead_time={:.1}s), {} children",
            self.max_lead_time_sec,
            output.len()
        );

        // Macro helper pour envoyer à tous les enfants
        macro_rules! send_to_children {
            ($segment:expr) => {
                for tx in &output {
                    tx.send($segment.clone())
                        .await
                        .map_err(|_| AudioError::ChildDied)?;
                }
            };
        }

        loop {
            let segment = tokio::select! {
                _ = stop_token.cancelled() => {
                    tracing::debug!("TimerNodeLogic cancelled");
                    break;
                }

                result = rx.recv() => {
                    match result {
                        Some(seg) => seg,
                        None => {
                            tracing::debug!("TimerNodeLogic received EOF");
                            break;
                        }
                    }
                }
            };

            // Traitement selon le type de segment
            match &segment.segment {
                _AudioSegment::Sync(marker) => {
                    match &**marker {
                        SyncMarker::TopZeroSync => {
                            // Reset le timer de référence
                            self.start_time = Some(Instant::now());
                            tracing::debug!("TimerNodeLogic: TopZeroSync received, timer reset");
                        }
                        _ => {
                            // Autres markers: passthrough transparent
                        }
                    }
                    send_to_children!(segment);
                }

                _AudioSegment::Chunk(_) => {
                    // Vérifier le pacing seulement si on a un timer de référence
                    if let Some(start) = self.start_time {
                        let chunk_timestamp = segment.timestamp_sec;
                        let elapsed = start.elapsed().as_secs_f64();
                        let lead_time = chunk_timestamp - elapsed;

                        if lead_time > self.max_lead_time_sec {
                            // On est trop en avance, attendre
                            let sleep_duration = lead_time - self.max_lead_time_sec;
                            tracing::trace!(
                                "TimerNodeLogic: SLEEPING {:.3}s (lead_time={:.3}s > max={:.1}s)",
                                sleep_duration,
                                lead_time,
                                self.max_lead_time_sec
                            );

                            tokio::select! {
                                _ = tokio::time::sleep(Duration::from_secs_f64(sleep_duration)) => {}
                                _ = stop_token.cancelled() => {
                                    tracing::debug!("TimerNodeLogic cancelled during sleep");
                                    break;
                                }
                            }
                        } else if lead_time < -0.5 {
                            // On est en retard de plus de 500ms, log warning
                            tracing::warn!(
                                "TimerNodeLogic: lagging behind by {:.3}s (chunk ts={:.3}s, elapsed={:.3}s)",
                                -lead_time,
                                chunk_timestamp,
                                elapsed
                            );
                        }
                    } else {
                        // Pas encore de TopZeroSync reçu, passthrough sans pacing
                        tracing::warn!("TimerNodeLogic: NO TIMER SET - passthrough without pacing! (ts={:.3}s)", segment.timestamp_sec);
                    }

                    send_to_children!(segment);
                }
            }
        }

        tracing::debug!("TimerNodeLogic::process finished");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TimerNode - Wrapper utilisant Node<TimerNodeLogic>
// ═══════════════════════════════════════════════════════════════════════════

pub struct TimerNode {
    inner: Node<TimerNodeLogic>,
}

impl TimerNode {
    /// Crée un TimerNode avec une avance maximale tolérée
    ///
    /// # Arguments
    ///
    /// * `max_lead_time_sec` - Avance maximale en secondes (ex: 3.0 pour 3s de buffer)
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use pmoaudio::TimerNode;
    ///
    /// // Tolérer 3 secondes d'avance
    /// let timer = TimerNode::new(3.0);
    /// ```
    pub fn new(max_lead_time_sec: f64) -> Self {
        Self::with_channel_size(max_lead_time_sec, DEFAULT_CHANNEL_SIZE)
    }

    /// Crée un TimerNode avec une taille de buffer MPSC personnalisée
    ///
    /// # Arguments
    ///
    /// * `max_lead_time_sec` - Avance maximale en secondes
    /// * `channel_size` - Taille du buffer MPSC (nombre de segments en attente)
    pub fn with_channel_size(max_lead_time_sec: f64, channel_size: usize) -> Self {
        let logic = TimerNodeLogic::new(max_lead_time_sec);
        Self {
            inner: Node::new_with_input(logic, channel_size),
        }
    }
}

#[async_trait::async_trait]
impl AudioPipelineNode for TimerNode {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.inner.get_tx()
    }

    fn register(&mut self, child: Box<dyn AudioPipelineNode>) {
        self.inner.register(child);
    }

    async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError> {
        Box::new(self.inner).run(stop_token).await
    }
}

impl TypedAudioNode for TimerNode {
    fn input_type(&self) -> Option<TypeRequirement> {
        // Accepte n'importe quel type
        Some(TypeRequirement::any())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        // Passthrough: produit le même type qu'il consomme
        Some(TypeRequirement::any())
    }
}
