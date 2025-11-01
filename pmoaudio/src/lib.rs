#![cfg_attr(feature = "simd", feature(portable_simd))]
#![doc = r#"
PMOAudio - Pipeline audio stéréo async optimisé

Cette crate fournit un pipeline audio push-based async utilisant Tokio,
optimisé pour minimiser les clonages de données via `Arc<[[i32; 2]]>`.

# Architecture

Le pipeline est composé de nodes asynchrones qui communiquent via des channels Tokio.
Les données audio sont encapsulées dans des [`AudioChunk`] et partagées via `Arc` pour
éviter les copies inutiles.

## Pipeline type

```text
SourceNode → DecoderNode → DSPNode → BufferNode → TimerNode → SinkNode(s)
                                          ↓
                                   Multiroom Sinks
                                   (avec offsets)
```

# Exemples

## Pipeline simple

```no_run
use pmoaudio::{SinkNode, SourceNode, TimerNode};

#[tokio::main]
async fn main() {
    let (mut timer, timer_tx) = TimerNode::new(10);
    let (sink, sink_tx) = SinkNode::new("Output".to_string(), 10);

    timer.add_subscriber(sink_tx);

    tokio::spawn(async move { timer.run().await.unwrap() });
    let sink_handle = tokio::spawn(async move {
        sink.run_with_stats().await.unwrap()
    });

    tokio::spawn(async move {
        let mut source = SourceNode::new();
        source.add_subscriber(timer_tx);
        source.generate_chunks(30, 4800, 48000, 440.0).await.unwrap();
    });

    sink_handle.await.unwrap();
}
```

## Configuration multiroom

```no_run
use pmoaudio::{BufferNode, SinkNode};

#[tokio::main]
async fn main() {
    let (buffer, buffer_tx) = BufferNode::new(50, 10);

    let (sink1, sink1_tx) = SinkNode::new("Room 1".to_string(), 10);
    let (sink2, sink2_tx) = SinkNode::new("Room 2".to_string(), 10);

    // Room 1 sans délai, Room 2 avec 5 chunks de retard
    buffer.add_subscriber_with_offset(sink1_tx, 0).await;
    buffer.add_subscriber_with_offset(sink2_tx, 5).await;

    tokio::spawn(async move { buffer.run().await.unwrap() });
    // ... spawn sinks et source
}
```

# Optimisations

- **Zero-copy** : Les [`AudioChunk`] sont partagés via `Arc`, seul le pointeur est cloné
- **Copy-on-Write** : Les nodes DSP clonent les données uniquement si modification nécessaire
- **Backpressure** : Channels bounded avec `try_send` pour éviter les blocages
- **RwLock** : Pour partage concurrent du compteur [`TimerNode`]
"#]
#[cfg(feature = "simd")]
use std::simd::*;

mod audio_chunk;
mod audio_segment;
pub mod conversions;
pub mod events;
pub mod nodes;
mod sample_types;
mod sync_marker;
pub mod type_constraints;
#[macro_use]
mod macros;

pub mod bit_depth;
pub mod dsp;

pub use audio_segment::{AudioSegment, _AudioSegment};
pub use sync_marker::SyncMarker;

pub use audio_chunk::{
    gain_db_from_linear, gain_linear_from_db, AudioChunk, AudioChunkData, AudioFloatChunk,
    AudioIntegerChunk,
};
pub use bit_depth::{Bit16, Bit24, Bit32, Bit8, BitDepth};
pub use sample_types::{Sample, I24};
pub use type_constraints::{
    check_compatibility, SampleType, TypeCategory, TypeMismatch, TypeRequirement,
};

pub use events::{
    AudioDataEvent, EventPublisher, EventReceiver, NodeEvent, NodeListener, SourceNameUpdateEvent,
    VolumeChangeEvent,
};

// Exports publics des nodes
pub use nodes::{
    converter_nodes::{ToF32Node, ToF64Node, ToI16Node, ToI24Node, ToI32Node},
    file_source::FileSource,
    flac_file_sink::{FlacFileSink, FlacFileSinkStats},
    http_source::HttpSource,
    AudioError, AudioNode, MultiSubscriberNode, SingleSubscriberNode, TypedAudioNode,
};

// Nodes temporairement désactivés
/*
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
};
*/
