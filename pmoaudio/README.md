# pmoaudio

**Pipeline audio stéréo asynchrone pour Rust**, conçu pour le streaming temps réel et le traitement audio multiformat.

## Caractéristiques principales

- 🎵 **Support multi-types** : I16, I24, I32, F32, F64 avec conversions optimisées SIMD
- ⚡ **Zero-Copy** : Partage des données via `Arc<[[T; 2]]>` pour minimiser les allocations
- 🔄 **Pipeline asynchrone** : Architecture basée sur Tokio avec nodes modulaires
- 🎚️ **Gestion du gain** : Copy-on-Write pour un contrôle de volume efficace
- 🌊 **Backpressure** : Canaux MPSC bornés pour éviter la saturation mémoire
- 🎯 **Type-safe** : Vérification de compatibilité des types entre nodes
- 🚀 **Optimisations SIMD** : ARM NEON, x86_64 AVX2, fallback scalaire

## Architecture

```
Source → [Processeur] → [Processeur] → Sink
  ↓          ↓              ↓           ↓
HttpSource  ToF32Node   TimerNode   FlacFileSink
FileSource  ToI32Node   Resampling  StreamingFlacSink
            Converter              AudioSink
```

### Types de Nodes

**Sources** : Génèrent des `AudioSegment`
- **HttpSource** : Téléchargement et décodage HTTP (FLAC, MP3, OGG, WAV, AIFF)
- **FileSource** : Lecture depuis fichiers locaux
- **PlaylistSource** (pmoaudio-ext) : Lecture depuis playlist avec cache

**Processeurs** : Transforment les segments audio
- **ToI16Node, ToI24Node, ToI32Node, ToF32Node, ToF64Node** : Conversions de type
- **ResamplingNode** : Rééchantillonnage (libsoxr)
- **TimerNode** : Rate-limiting pour éviter la saturation

**Sinks** : Consomment les segments audio
- **FlacFileSink** : Encodage et écriture FLAC
- **AudioSink** : Collecte en mémoire (tests)
- **FlacCacheSink** (pmoaudio-ext) : Cache avec cover art
- **StreamingFlacSink** (pmoaudio-ext) : Stream FLAC multi-clients HTTP
- **StreamingOggFlacSink** (pmoaudio-ext) : Stream OGG-FLAC avec métadonnées

## Installation

```toml
[dependencies]
pmoaudio = { path = "../pmoaudio" }
pmoaudio-ext = { path = "../pmoaudio-ext", features = ["http-stream"] }
```

## Exemple simple

```rust
use pmoaudio::{FileSource, ToF32Node, AudioPipelineNode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = FileSource::new("music.flac");
    let converter = ToF32Node::new();

    source.register(converter);

    let handle = source.start();
    handle.wait().await?;

    Ok(())
}
```

## Exemple avec streaming HTTP

```rust
use pmoaudio::{HttpSource, TimerNode};
use pmoaudio_ext::sinks::StreamingFlacSink;
use pmoflac::EncoderOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Pipeline: HTTP Source → TimerNode → Streaming FLAC Sink
    let mut http_source = HttpSource::new("https://api.radioparadise.com/...");
    let mut timer = TimerNode::new(3.0);  // 3s max lead time

    let (flac_sink, stream_handle) = StreamingFlacSink::new(
        EncoderOptions::default(),
        16,  // bits per sample
    );

    http_source.register(Box::new(timer));
    timer.register(Box::new(flac_sink));

    let pipeline = http_source.start();

    // Servir aux clients HTTP
    let stream = stream_handle.subscribe_flac();
    // ... utiliser avec tokio_util::io::ReaderStream

    pipeline.wait().await?;
    Ok(())
}
```

## Types de données

### AudioChunk

Enum pour tous les types d'échantillons supportés :

```rust
pub enum AudioChunk {
    I16(Arc<AudioChunkData<i16>>),
    I24(Arc<AudioChunkData<I24>>),
    I32(Arc<AudioChunkData<i32>>),
    F32(Arc<AudioChunkData<f32>>),
    F64(Arc<AudioChunkData<f64>>),
}
```

### AudioSegment

Wrapper autour d'un chunk audio ou d'un marqueur de synchronisation :

```rust
pub struct AudioSegment {
    pub order: u64,
    pub timestamp_sec: f64,
    pub segment: _AudioSegment,  // Chunk ou SyncMarker
}
```

### SyncMarker

Marqueurs pour événements du pipeline :

```rust
pub enum SyncMarker {
    TopZeroSync,                    // Début du stream
    TrackBoundary { metadata },     // Changement de piste
    StreamMetadata { key, value },  // Métadonnées
    Heartbeat,                      // Keep-alive
    EndOfStream,                    // Fin
    Error(String),                  // Erreur
}
```

## Conversions et DSP

Le module `dsp` fournit des fonctions optimisées SIMD :

```rust
use pmoaudio::dsp::{bitdepth_change_stereo, apply_gain_stereo_i32};

// Conversion bit-depth
let mut data = vec![[1000i32, 2000i32]];
bitdepth_change_stereo(&mut data, BitDepth::B16, BitDepth::B24);

// Application de gain
let mut data = vec![[100000i32, 200000i32]];
apply_gain_stereo_i32(&mut data, 6.0); // +6dB
```

## Documentation complète

Pour une documentation détaillée de l'architecture, consultez [ARCHITECTURE.md](ARCHITECTURE.md) qui couvre :

- Types de données et leur cycle de vie
- Architecture complète du pipeline (5 phases)
- Système de gestion du gain (Copy-on-Write)
- Type Constraints System
- Optimisations SIMD et performances
- Streaming HTTP et backpressure
- Exemples de pipelines complets

## Tests

```bash
# Tests unitaires
cargo test --package pmoaudio --lib

# Exemples
cargo run --package pmoaudio --example audio_chunk_api
```

**Couverture** : 35+ tests unitaires couvrant tous les modules critiques

## Performances

Sur un CPU moderne (2023) :
- **Décodage FLAC** : ~200-400× temps réel
- **Encodage FLAC** : ~50-100× temps réel
- **Conversion de type** : ~1000× temps réel
- **Rééchantillonnage** : ~100× temps réel (quality=high)

**Latence end-to-end** : ~1-2 secondes (streaming HTTP)

## Extensions (pmoaudio-ext)

Le crate `pmoaudio-ext` fournit des nodes avancés :

### Features disponibles
- `cache-sink` : FlacCacheSink avec gestion de cover art
- `playlist` : PlaylistSource pour lecture depuis playlists
- `http-stream` : StreamingFlacSink et StreamingOggFlacSink pour diffusion HTTP
- `all` : Active toutes les features

```toml
[dependencies]
pmoaudio-ext = { path = "../pmoaudio-ext", features = ["http-stream"] }
```

## Structure du projet

```
pmoaudio/
├── src/
│   ├── audio_chunk.rs      # Types AudioChunk et AudioChunkData<T>
│   ├── audio_segment.rs    # Wrapper avec timestamps et sync markers
│   ├── bit_depth.rs        # Gestion des profondeurs de bit
│   ├── conversions.rs      # Conversions entre types optimisées
│   ├── sample_types.rs     # Trait Sample et type I24
│   ├── sync_marker.rs      # Marqueurs de synchronisation
│   ├── events.rs           # Système d'événements générique
│   ├── pipeline.rs         # Orchestration du pipeline
│   ├── type_constraints.rs # Vérification de compatibilité des types
│   ├── macros.rs           # Macros utilitaires
│   ├── dsp/                # Fonctions DSP optimisées SIMD
│   │   ├── depth.rs        # Conversion bit-depth
│   │   ├── gain_*.rs       # Application de gain
│   │   ├── int_float.rs    # Conversions int↔float
│   │   └── resampling.rs   # Rééchantillonnage
│   └── nodes/              # Nodes du pipeline
│       ├── http_source.rs
│       ├── file_source.rs
│       ├── timer_node.rs
│       ├── flac_file_sink.rs
│       ├── resampling_node.rs
│       └── converter_nodes.rs
├── examples/               # Exemples d'utilisation
├── ARCHITECTURE.md         # Documentation détaillée
└── README.md              # Ce fichier

pmoaudio-ext/
├── src/
│   ├── sinks/
│   │   ├── flac_cache_sink.rs
│   │   ├── streaming_flac_sink.rs
│   │   └── streaming_ogg_flac_sink.rs
│   └── sources/
│       └── playlist_source.rs
└── Cargo.toml
```

## Dépendances principales

- `tokio` : Runtime async
- `tokio-util` : Utilitaires async
- `async-trait` : Traits async
- `reqwest` : Client HTTP
- `soxr` : Rééchantillonnage
- `pmoflac` : Encodage/décodage FLAC
- `pmometadata` : Gestion des métadonnées

## Historique

Les documents historiques (anciens refactorings, implémentations obsolètes) sont archivés dans [`docs/historical/`](docs/historical/).

## Licence

CeCill-2.0 (compatible GPL)

## Contributeurs

Projet PMOMusic - Streaming audio multiroom pour Rust
