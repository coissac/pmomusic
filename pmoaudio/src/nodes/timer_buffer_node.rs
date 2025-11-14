//! TimerBufferNode - Maintient un tampon temporel capacitif avant diffusion
//!
//! Ce node implémente un buffer capacitif qui accumule un temps configurable
//! de données audio avant de les diffuser. Une fois le buffer rempli, il
//! maintient ce niveau en diffusant les données au même rythme qu'elles arrivent.
//!
//! # Use Cases
//!
//! - **Buffering initial**: Accumule N secondes de données avant de commencer la lecture
//! - **Smoothing**: Absorbe les variations de débit entre source et sink
//! - **Streaming**: Pré-charge un buffer pour éviter les coupures
//!
//! # Exemple
//!
//! ```no_run
//! use pmoaudio::{HttpSource, TimerBufferNode, AudioSink};
//!
//! let mut source = HttpSource::new(url);
//! let mut buffer = TimerBufferNode::new(3.0); // Buffer 3s avant de commencer
//! let mut sink = AudioSink::new();
//!
//! source.register(Box::new(buffer));
//! buffer.register(Box::new(sink));
//! ```
//!
//! # Architecture
//!
//! ```text
//! HttpSource → TimerBufferNode → AudioSink
//!      ↓              ↓              ↓
//!  Flux réseau   Buffer 3s     Lecture stable
//!  variable      capacitif     sans coupures
//! ```
//!
//! Le TimerBufferNode:
//! 1. Accumule les chunks dans un buffer jusqu'à atteindre `capacity_sec`
//! 2. Une fois plein, diffuse les chunks en mode FIFO
//! 3. Maintient un niveau constant d'environ `capacity_sec` secondes
//!
//! # Markers Supportés
//!
//! - **TopZeroSync**: Vide le buffer et reset le compteur
//! - **TrackBoundary**: Passthrough transparent
//! - **Heartbeat**: Passthrough transparent
//! - **EndOfStream**: Flush le buffer restant avant propagation
//!
//! # Performance
//!
//! - **CPU**: Minimal (VecDeque efficace)
//! - **Latency**: Ajoute `capacity_sec` de buffering initial
//! - **Memory**: Proportionnel à `capacity_sec` (ex: ~3MB pour 3s @ 48kHz stéréo)

use crate::{
    nodes::{AudioError, TypedAudioNode, DEFAULT_CHANNEL_SIZE, DEFAULT_CHUNK_DURATION_MS},
    pipeline::{AudioPipelineNode, Node, NodeLogic},
    type_constraints::TypeRequirement,
    AudioSegment, SyncMarker, _AudioSegment,
};
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

// ═══════════════════════════════════════════════════════════════════════════
// TimerBufferNodeLogic - Logique pure de buffering capacitif
// ═══════════════════════════════════════════════════════════════════════════

/// Logique pure de buffering temporel capacitif
///
/// Maintient un buffer de taille fixe (en secondes) et diffuse les segments
/// en mode FIFO une fois le buffer rempli.
pub struct TimerBufferNodeLogic {
    /// Capacité du buffer en secondes
    capacity_sec: f64,
    /// Temps actuellement bufferisé en secondes
    buffered_time_sec: f64,
    /// Durée par défaut d'un chunk (fallback)
    default_chunk_duration_sec: f64,
    /// Timestamp du chunk précédent (pour estimer les durées)
    prev_input_ts: Option<f64>,
    /// Buffer FIFO de segments avec leurs durées
    buffer: VecDeque<(Arc<AudioSegment>, f64)>,
    /// Nombre de chunks traités (pour instrumentation)
    chunk_count: u64,
    /// Nombre de chunks flushés (pour instrumentation)
    flush_count: u64,
    /// Dernier log d'instrumentation
    last_stats_log: Option<Instant>,
}

impl TimerBufferNodeLogic {
    pub fn new(capacity_sec: f64) -> Self {
        Self {
            capacity_sec: capacity_sec.max(0.0),
            buffered_time_sec: 0.0,
            default_chunk_duration_sec: DEFAULT_CHUNK_DURATION_MS / 1000.0,
            prev_input_ts: None,
            buffer: VecDeque::new(),
            chunk_count: 0,
            flush_count: 0,
            last_stats_log: None,
        }
    }

    /// Estime la durée d'un chunk basé sur le delta de timestamps
    fn estimate_duration(&mut self, ts: f64) -> f64 {
        if let Some(prev) = self.prev_input_ts {
            let delta = (ts - prev).clamp(0.0, 10.0);
            self.prev_input_ts = Some(ts);
            if delta == 0.0 {
                self.default_chunk_duration_sec
            } else {
                delta
            }
        } else {
            self.prev_input_ts = Some(ts);
            self.default_chunk_duration_sec
        }
    }

    /// Flush un segment du buffer vers les outputs
    async fn flush_one(
        &mut self,
        output: &[mpsc::Sender<Arc<AudioSegment>>],
    ) -> Result<(), AudioError> {
        if let Some((segment, duration)) = self.buffer.pop_front() {
            self.flush_count += 1;
            self.buffered_time_sec = (self.buffered_time_sec - duration).max(0.0);

            tracing::trace!(
                "TimerBufferNode: flushing segment (ts={:.3}s, duration={:.3}s, remaining={:.3}s, {} items in buffer)",
                segment.timestamp_sec,
                duration,
                self.buffered_time_sec,
                self.buffer.len()
            );

            for tx in output {
                tx.send(segment.clone())
                    .await
                    .map_err(|_| AudioError::ChildDied)?;
            }
        }
        Ok(())
    }

    fn maybe_log_stats(&mut self) {
        let now = Instant::now();
        let should_log = match self.last_stats_log {
            None => true,
            Some(last) => now.duration_since(last).as_secs() >= 1,
        };

        if should_log {
            self.last_stats_log = Some(now);
            tracing::debug!(
                "TimerBufferNode stats: chunks_received={} chunks_flushed={} buffered={:.3}s capacity={:.3}s buffer_items={}",
                self.chunk_count,
                self.flush_count,
                self.buffered_time_sec,
                self.capacity_sec,
                self.buffer.len()
            );
        }
    }
}

#[async_trait::async_trait]
impl NodeLogic for TimerBufferNodeLogic {
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let mut rx = input.expect("TimerBufferNode must have input");
        tracing::info!(
            "TimerBufferNodeLogic::process started (capacity={:.1}s), {} children",
            self.capacity_sec,
            output.len()
        );

        loop {
            // ╔═══════════════════════════════════════════════════════════════╗
            // ║ LOGIQUE CAPACITIVE PAR BACKPRESSURE NATURELLE                 ║
            // ║                                                               ║
            // ║ Si le buffer >= capacity, on flush en continu (boucle)       ║
            // ║ sans recevoir de nouveaux segments. Cela force la            ║
            // ║ backpressure en amont si le sink en aval est lent.           ║
            // ╚═══════════════════════════════════════════════════════════════╝
            if self.buffered_time_sec >= self.capacity_sec && !self.buffer.is_empty() {
                self.flush_one(&output).await?;
                continue;
            }

            let segment = tokio::select! {
                _ = stop_token.cancelled() => {
                    tracing::debug!("TimerBufferNode cancelled");
                    break;
                }

                result = rx.recv() => {
                    match result {
                        Some(seg) => seg,
                        None => {
                            tracing::debug!("TimerBufferNode received EOF");
                            break;
                        }
                    }
                }
            };

            match &segment.segment {
                _AudioSegment::Sync(marker) => {
                    match &**marker {
                        SyncMarker::TopZeroSync => {
                            // Reset le buffer complètement
                            self.buffer.clear();
                            self.buffered_time_sec = 0.0;
                            self.prev_input_ts = Some(0.0);
                            self.chunk_count = 0;
                            self.flush_count = 0;
                            tracing::debug!("TimerBufferNode: TopZeroSync received, buffer reset");
                        }
                        _ => {
                            // Autres markers: passthrough transparent
                        }
                    }

                    // Propager le marker immédiatement
                    for tx in &output {
                        tx.send(segment.clone())
                            .await
                            .map_err(|_| AudioError::ChildDied)?;
                    }
                }

                _AudioSegment::Chunk(chunk) => {
                    self.chunk_count += 1;

                    // Calculer la durée du chunk
                    let frames = chunk.len() as f64;
                    let sample_rate = chunk.sample_rate() as f64;
                    let duration = if frames > 0.0 && sample_rate > 0.0 {
                        frames / sample_rate
                    } else {
                        self.estimate_duration(segment.timestamp_sec)
                    };

                    tracing::trace!(
                        "TimerBufferNode: received chunk (ts={:.3}s, duration={:.3}s, buffered={:.3}s, capacity={:.3}s)",
                        segment.timestamp_sec,
                        duration,
                        self.buffered_time_sec,
                        self.capacity_sec
                    );

                    // Ajouter le chunk au buffer
                    self.buffer.push_back((segment.clone(), duration));
                    self.buffered_time_sec += duration;

                    // ╔═══════════════════════════════════════════════════════════╗
                    // ║ FLUSH IMMÉDIAT : Vider aussi vite que possible           ║
                    // ║                                                           ║
                    // ║ Le send() bloquera si le sink est lent, créant           ║
                    // ║ naturellement la backpressure. Le buffer se remplit      ║
                    // ║ pendant que send() attend, jusqu'à atteindre capacity.   ║
                    // ╚═══════════════════════════════════════════════════════════╝
                    self.flush_one(&output).await?;

                    self.maybe_log_stats();
                }
            }
        }

        // EOF reçu, flusher le buffer restant
        tracing::info!(
            "TimerBufferNode: EOF received, flushing remaining buffer ({:.3}s, {} items)",
            self.buffered_time_sec,
            self.buffer.len()
        );
        while !self.buffer.is_empty() {
            self.flush_one(&output).await?;
        }

        tracing::debug!("TimerBufferNodeLogic::process finished");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TimerBufferNode - Wrapper utilisant Node<TimerBufferNodeLogic>
// ═══════════════════════════════════════════════════════════════════════════

pub struct TimerBufferNode {
    inner: Node<TimerBufferNodeLogic>,
}

impl TimerBufferNode {
    /// Crée un TimerBufferNode avec une capacité donnée
    ///
    /// # Arguments
    ///
    /// * `capacity_sec` - Capacité du buffer en secondes (ex: 3.0 pour 3s)
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use pmoaudio::TimerBufferNode;
    ///
    /// // Buffer 3 secondes avant de commencer la diffusion
    /// let buffer = TimerBufferNode::new(3.0);
    /// ```
    pub fn new(capacity_sec: f64) -> Self {
        Self::with_channel_size(capacity_sec, DEFAULT_CHANNEL_SIZE)
    }

    /// Crée un TimerBufferNode avec une taille de buffer MPSC personnalisée
    ///
    /// # Arguments
    ///
    /// * `capacity_sec` - Capacité du buffer en secondes
    /// * `channel_size` - Taille du buffer MPSC (nombre de segments en attente)
    pub fn with_channel_size(capacity_sec: f64, channel_size: usize) -> Self {
        let logic = TimerBufferNodeLogic::new(capacity_sec);
        Self {
            inner: Node::new_with_input(logic, channel_size),
        }
    }
}

#[async_trait::async_trait]
impl AudioPipelineNode for TimerBufferNode {
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

impl TypedAudioNode for TimerBufferNode {
    fn input_type(&self) -> Option<TypeRequirement> {
        // Accepte n'importe quel type
        Some(TypeRequirement::any())
    }

    fn output_type(&self) -> Option<TypeRequirement> {
        // Passthrough: produit le même type qu'il consomme
        Some(TypeRequirement::any())
    }
}
