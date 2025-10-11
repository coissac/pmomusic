//! PMOAudio - Pipeline audio stéréo async optimisé
//!
//! Cette crate fournit un pipeline audio push-based async utilisant Tokio,
//! optimisé pour minimiser les clonages de données via `Arc<Vec<f32>>`.
//!
//! # Architecture
//!
//! Le pipeline est composé de nodes asynchrones qui communiquent via des channels Tokio.
//! Les données audio sont encapsulées dans des [`AudioChunk`] et partagées via `Arc` pour
//! éviter les copies inutiles.
//!
//! ## Pipeline type
//!
//! ```text
//! SourceNode → DecoderNode → DSPNode → BufferNode → TimerNode → SinkNode(s)
//!                                           ↓
//!                                    Multiroom Sinks
//!                                    (avec offsets)
//! ```
//!
//! # Exemples
//!
//! ## Pipeline simple
//!
//! ```no_run
//! use pmoaudio::{SinkNode, SourceNode, TimerNode};
//!
//! #[tokio::main]
//! async fn main() {
//!     let (mut timer, timer_tx) = TimerNode::new(10);
//!     let (sink, sink_tx) = SinkNode::new("Output".to_string(), 10);
//!
//!     timer.add_subscriber(sink_tx);
//!
//!     tokio::spawn(async move { timer.run().await.unwrap() });
//!     let sink_handle = tokio::spawn(async move {
//!         sink.run_with_stats().await.unwrap()
//!     });
//!
//!     tokio::spawn(async move {
//!         let mut source = SourceNode::new();
//!         source.add_subscriber(timer_tx);
//!         source.generate_chunks(30, 4800, 48000, 440.0).await.unwrap();
//!     });
//!
//!     sink_handle.await.unwrap();
//! }
//! ```
//!
//! ## Configuration multiroom
//!
//! ```no_run
//! use pmoaudio::{BufferNode, SinkNode};
//!
//! #[tokio::main]
//! async fn main() {
//!     let (buffer, buffer_tx) = BufferNode::new(50, 10);
//!
//!     let (sink1, sink1_tx) = SinkNode::new("Room 1".to_string(), 10);
//!     let (sink2, sink2_tx) = SinkNode::new("Room 2".to_string(), 10);
//!
//!     // Room 1 sans délai, Room 2 avec 5 chunks de retard
//!     buffer.add_subscriber_with_offset(sink1_tx, 0).await;
//!     buffer.add_subscriber_with_offset(sink2_tx, 5).await;
//!
//!     tokio::spawn(async move { buffer.run().await.unwrap() });
//!     // ... spawn sinks et source
//! }
//! ```
//!
//! # Optimisations
//!
//! - **Zero-copy** : Les [`AudioChunk`] sont partagés via `Arc`, seul le pointeur est cloné
//! - **Copy-on-Write** : Les nodes DSP clonent les données uniquement si modification nécessaire
//! - **Backpressure** : Channels bounded avec `try_send` pour éviter les blocages
//! - **RwLock** : Pour partage concurrent du compteur [`TimerNode`]

mod audio_chunk;
mod nodes;
pub mod events;

pub use audio_chunk::AudioChunk;
pub use events::{
    AudioDataEvent, EventPublisher, EventReceiver, NodeEvent, NodeListener,
    SourceNameUpdateEvent, VolumeChangeEvent,
};
pub use nodes::{
    buffer_node::BufferNode,
    chromecast_sink::{ChromecastConfig, ChromecastSink, ChromecastStats, StreamEncoding},
    decoder_node::DecoderNode,
    disk_sink::{AudioFileFormat, DiskSink, DiskSinkConfig, DiskSinkStats},
    dsp_node::DspNode,
    mpd_sink::{MpdAudioFormat, MpdConfig, MpdHandle, MpdSink, MpdStats},
    sink_node::{SinkNode, SinkStats},
    source_node::SourceNode,
    timer_node::{TimerHandle, TimerNode},
    volume_node::{HardwareVolumeNode, VolumeHandle, VolumeNode},
    AudioError, AudioNode, MultiSubscriberNode, SingleSubscriberNode,
};
