//! Node statistics tracking
//!
//! Provides detailed statistics for pipeline nodes to understand
//! data flow, backpressure behavior, and timing.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Statistics pour un node audio
#[derive(Debug)]
pub struct NodeStats {
    /// Nom du node pour identification
    pub name: String,

    /// Instant de démarrage du node
    pub start_time: Instant,

    /// Nombre total de segments reçus
    pub segments_received: AtomicUsize,

    /// Nombre total de segments envoyés
    pub segments_sent: AtomicUsize,

    /// Nombre total de bytes traités
    pub bytes_processed: AtomicU64,

    /// Nombre de fois où l'envoi a été bloqué (backpressure)
    pub backpressure_blocks: AtomicUsize,

    /// Temps total passé bloqué en millisecondes
    pub backpressure_time_ms: AtomicU64,

    /// Timestamp du premier segment (secondes)
    pub first_segment_timestamp: AtomicU64, // Stocké comme u64 * 1000 pour précision

    /// Timestamp du dernier segment (secondes)
    pub last_segment_timestamp: AtomicU64, // Stocké comme u64 * 1000 pour précision
}

impl NodeStats {
    pub fn new(name: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            name: name.into(),
            start_time: Instant::now(),
            segments_received: AtomicUsize::new(0),
            segments_sent: AtomicUsize::new(0),
            bytes_processed: AtomicU64::new(0),
            backpressure_blocks: AtomicUsize::new(0),
            backpressure_time_ms: AtomicU64::new(0),
            first_segment_timestamp: AtomicU64::new(u64::MAX),
            last_segment_timestamp: AtomicU64::new(0),
        })
    }

    /// Enregistre la réception d'un segment
    pub fn record_segment_received(&self, timestamp_sec: f64) {
        self.segments_received.fetch_add(1, Ordering::Relaxed);

        let ts_millis = (timestamp_sec * 1000.0) as u64;

        // Update first timestamp (atomic min)
        let mut current = self.first_segment_timestamp.load(Ordering::Relaxed);
        while current > ts_millis {
            match self.first_segment_timestamp.compare_exchange_weak(
                current,
                ts_millis,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current = x,
            }
        }

        // Update last timestamp (atomic max)
        let mut current = self.last_segment_timestamp.load(Ordering::Relaxed);
        while current < ts_millis {
            match self.last_segment_timestamp.compare_exchange_weak(
                current,
                ts_millis,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current = x,
            }
        }
    }

    /// Enregistre l'envoi d'un segment
    pub fn record_segment_sent(&self, bytes: usize) {
        self.segments_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_processed.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// Enregistre un événement de backpressure
    pub fn record_backpressure(&self, duration_ms: u64) {
        self.backpressure_blocks.fetch_add(1, Ordering::Relaxed);
        self.backpressure_time_ms.fetch_add(duration_ms, Ordering::Relaxed);
    }

    /// Retourne un rapport formaté des statistiques
    pub fn report(&self) -> String {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let received = self.segments_received.load(Ordering::Relaxed);
        let sent = self.segments_sent.load(Ordering::Relaxed);
        let bytes = self.bytes_processed.load(Ordering::Relaxed);
        let bp_blocks = self.backpressure_blocks.load(Ordering::Relaxed);
        let bp_time_ms = self.backpressure_time_ms.load(Ordering::Relaxed);

        let first_ts = self.first_segment_timestamp.load(Ordering::Relaxed);
        let last_ts = self.last_segment_timestamp.load(Ordering::Relaxed);

        let first_ts_sec = if first_ts == u64::MAX { 0.0 } else { first_ts as f64 / 1000.0 };
        let last_ts_sec = last_ts as f64 / 1000.0;
        let audio_duration = last_ts_sec - first_ts_sec;

        let mb = bytes as f64 / 1_048_576.0;
        let throughput_mbps = if elapsed > 0.0 { mb / elapsed } else { 0.0 };

        format!(
            "[{}]\n\
             Elapsed: {:.1}s | Received: {} | Sent: {} | Lost: {}\n\
             Data: {:.1} MB | Throughput: {:.2} MB/s\n\
             Audio: {:.1}s (first: {:.1}s, last: {:.1}s) | Real-time ratio: {:.1}%\n\
             Backpressure: {} blocks, {:.2}s total ({:.1}% of time)",
            self.name,
            elapsed, received, sent, received.saturating_sub(sent),
            mb, throughput_mbps,
            audio_duration, first_ts_sec, last_ts_sec,
            if audio_duration > 0.0 { (elapsed / audio_duration) * 100.0 } else { 0.0 },
            bp_blocks, bp_time_ms as f64 / 1000.0,
            if elapsed > 0.0 { (bp_time_ms as f64 / 1000.0 / elapsed) * 100.0 } else { 0.0 }
        )
    }
}
