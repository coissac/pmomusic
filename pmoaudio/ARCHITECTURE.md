# Architecture du Pipeline Audio pmoaudio

Ce document décrit l'architecture générale du framework de pipeline audio **pmoaudio** et son extension **pmoaudio-ext**.

## Vue d'ensemble

**pmoaudio** est un framework async pour le traitement audio en temps réel basé sur une architecture de pipeline modulaire. Il utilise Tokio pour l'asynchronisme et optimise les performances en minimisant les copies de données grâce à `Arc<T>`.

### Principes de conception

1. **Zero-Copy** : Les données audio sont partagées via `Arc<[[T; 2]]>` pour éviter les copies coûteuses
2. **Copy-on-Write** : Les opérations DSP créent de nouvelles instances seulement si nécessaire
3. **Type-Safety** : Système de types complet pour garantir la compatibilité entre nodes
4. **Async-First** : Architecture basée sur Tokio avec des tasks concurrentes
5. **Backpressure** : Canaux MPSC bornés pour éviter la saturation mémoire

## Types de données fondamentaux

### AudioChunk

`AudioChunk` est l'unité de base de données audio. C'est un enum qui peut contenir différents types d'échantillons :

```rust
pub enum AudioChunk {
    I16(Arc<AudioChunkData<i16>>),   // 16-bit signed integer
    I24(Arc<AudioChunkData<I24>>),   // 24-bit signed integer (stocké dans i32)
    I32(Arc<AudioChunkData<i32>>),   // 32-bit signed integer
    F32(Arc<AudioChunkData<f32>>),   // 32-bit float [-1.0, 1.0]
    F64(Arc<AudioChunkData<f64>>),   // 64-bit float [-1.0, 1.0]
}
```

Chaque variant contient un `Arc<AudioChunkData<T>>` qui encapsule :
- Les frames stéréo `Arc<[[T; 2]]>` (Zero-Copy)
- Le sample rate (Hz)
- Le gain en dB (Copy-on-Write)

### AudioSegment

`AudioSegment` enveloppe soit un `AudioChunk`, soit un `SyncMarker` :

```rust
pub struct AudioSegment {
    pub order: u64,              // Numéro de séquence
    pub timestamp_sec: f64,      // Timestamp en secondes
    pub segment: _AudioSegment,  // Chunk ou Marker
}

pub enum _AudioSegment {
    Chunk(Arc<AudioChunk>),
    Sync(Arc<SyncMarker>),
}
```

### SyncMarker

Les marqueurs de synchronisation permettent de signaler des événements dans le pipeline :

```rust
pub enum SyncMarker {
    TopZeroSync,                                      // Début du stream
    TrackBoundary { metadata: Arc<RwLock<dyn TrackMetadata>> },  // Changement de piste
    StreamMetadata { key: String, value: String },   // Métadonnées de flux
    Heartbeat,                                        // Keep-alive
    EndOfStream,                                      // Fin du stream
    Error(String),                                    // Erreur
}
```

## Architecture du Pipeline

### Nodes et leur cycle de vie

Un pipeline est composé de **nodes** connectés entre eux via des canaux MPSC :

```
Source → [Processor] → [Processor] → Sink
```

Chaque node implémente le trait `AudioPipelineNode` :

```rust
#[async_trait]
pub trait AudioPipelineNode: Send + 'static {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>>;
    fn register(&mut self, child: Box<dyn AudioPipelineNode>);
    async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError>;
    fn start(self: Box<Self>) -> PipelineHandle;
}
```

### Node<L: NodeLogic>

La plupart des nodes utilisent la structure générique `Node<L>` qui implémente le pattern **Template Method** :

```rust
pub struct Node<L: NodeLogic> {
    logic: L,
    input_rx: Option<mpsc::Receiver<Arc<AudioSegment>>>,
    children: Vec<Box<dyn AudioPipelineNode>>,
}
```

Le trait `NodeLogic` contient la logique métier pure :

```rust
#[async_trait]
pub trait NodeLogic: Send + 'static {
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError>;

    async fn cleanup(&mut self, reason: StopReason) -> Result<(), AudioError>;
}
```

### Phases d'exécution d'un Node

Quand un node démarre via `start()`, il passe par 5 phases :

#### Phase 1 : Spawning des enfants
```rust
for child in self.children {
    let handle = child.start();
    child_handles.push(handle);
}
```

#### Phase 2 : Monitoring des enfants
```rust
tokio::select! {
    _ = child_handle.wait() => {
        // Enfant terminé prématurément
    }
    _ = stop_token.cancelled() => {
        // Annulation externe
    }
}
```

#### Phase 3 : Exécution de la logique
```rust
tokio::spawn(async move {
    logic.process(input, outputs, stop_token).await
});
```

#### Phase 4 : Cleanup
```rust
logic.cleanup(reason).await?;
```

#### Phase 5 : Retour du résultat
```rust
Ok(()) ou Err(AudioError)
```

## Types de Nodes

### Sources

Les sources produisent des `AudioSegment` sans consommer d'entrée :

- **HttpSource** : Télécharge et décode l'audio via HTTP (FLAC, MP3, OGG, WAV, AIFF)
- **FileSource** : Lit l'audio depuis des fichiers locaux
- **PlaylistSource** (pmoaudio-ext) : Lit depuis une playlist avec cache

Exemple de création :
```rust
let mut source = HttpSource::new("https://api.radioparadise.com/...");
```

### Processeurs

Les processeurs transforment les `AudioSegment` en transit :

- **ToI16Node, ToI24Node, ToI32Node, ToF32Node, ToF64Node** : Conversion de type
- **ResamplingNode** : Rééchantillonnage (via libsoxr)
- **TimerNode** : Rate-limiting pour éviter la saturation

Exemple de pipeline avec conversion :
```rust
let mut source = HttpSource::new(...);
let converter = ToF32Node::new();
source.register(converter);
```

### Sinks

Les sinks consomment les `AudioSegment` sans produire de sortie :

- **FlacFileSink** : Encode et écrit des fichiers FLAC
- **AudioSink** : Collecte en mémoire (pour tests)
- **FlacCacheSink** (pmoaudio-ext) : Encode et stocke dans le cache
- **StreamingFlacSink** (pmoaudio-ext) : Stream FLAC vers HTTP
- **StreamingOggFlacSink** (pmoaudio-ext) : Stream OGG-FLAC vers HTTP

## Gestion du Gain

Le gain est géré en **Copy-on-Write** :

```rust
let chunk = AudioChunkData::new(samples, 48000, 0.0);  // Gain = 0dB
let louder = chunk.set_gain_db(6.0);                   // Nouveau Arc, +6dB
let applied = louder.apply_gain();                     // Applique le gain in-place
```

Les méthodes de gain :
- `set_gain_db(db)` : Définit le gain absolu
- `with_modified_gain_db(delta)` : Ajoute un delta
- `apply_gain()` : Applique le gain et reset à 0dB

## Type Constraints System

Avant de connecter des nodes, on peut vérifier la compatibilité des types :

```rust
use pmoaudio::type_constraints::{check_compatibility, TypeRequirement, SampleType};

let source_output = TypeRequirement::specific(SampleType::I32);
let sink_input = TypeRequirement::any_integer();

match check_compatibility(&source_output, &sink_input) {
    Ok(()) => println!("Compatible!"),
    Err(e) => eprintln!("Incompatible: {}", e),
}
```

## Optimisations DSP

Le module `dsp/` fournit des fonctions optimisées SIMD :

### Bit-depth conversion
```rust
use pmoaudio::dsp::bitdepth_change_stereo;

let mut data = vec![[1000i32, 2000i32]];
bitdepth_change_stereo(&mut data, BitDepth::B16, BitDepth::B24);
// Décalage de 8 bits à gauche
```

### Gain application
```rust
use pmoaudio::dsp::apply_gain_stereo_i32;

let mut data = vec![[100000i32, 200000i32]];
apply_gain_stereo_i32(&mut data, 6.0); // +6dB (×2)
```

### Conversions int↔float
```rust
use pmoaudio::dsp::{i32_stereo_to_pairs_f32, pairs_f32_to_i32_stereo};

let int_data = vec![[1000000i32, 2000000i32]];
let float_data = i32_stereo_to_pairs_f32(&int_data);
let back_to_int = pairs_f32_to_i32_stereo(&float_data);
```

Les optimisations SIMD sont activées avec la feature `simd` et utilisent :
- **ARM NEON** : vqdmulhq_s32, vld1q_s32, vst1q_s32
- **x86_64 AVX2** : _mm256_mulhi_epi32, _mm256_loadu_si256
- **Fallback scalaire** : Pour les architectures sans SIMD

## Streaming HTTP (pmoaudio-ext)

### StreamingFlacSink

Architecture pour diffuser du FLAC vers plusieurs clients HTTP :

```
AudioSegment Pipeline
      ↓
StreamingFlacSink (convert to PCM)
      ↓
pmoflac::encode_flac_stream()
      ↓
Broadcaster Task (broadcast::channel)
      ↓
Multiple HTTP clients
  ├─ FLAC pur (renderers UPnP)
  └─ ICY-FLAC (avec métadonnées)
```

Utilisation :
```rust
let (sink, handle) = StreamingFlacSink::new(EncoderOptions::default(), 16);
source.register(Box::new(sink));

// Dans le HTTP handler
let stream = handle.subscribe_flac();
Body::from_stream(ReaderStream::new(stream))
```

### StreamingOggFlacSink

Similaire à StreamingFlacSink mais encapsule le FLAC dans OGG pour :
- Meilleure compatibilité avec certains clients
- Support des Vorbis Comments intégrés
- OGG chaining pour les changements de piste

## Backpressure et Pacing

### Backpressure via MPSC borné

Les canaux MPSC ont une capacité limitée (par défaut 16) :

```rust
pub const DEFAULT_CHANNEL_SIZE: usize = 16;
```

Quand le canal est plein, le `send()` bloque jusqu'à ce qu'un slot se libère.

### TimerNode pour rate-limiting

Le `TimerNode` empêche les sources rapides de saturer les sinks lents :

```rust
let mut timer = TimerNode::new(3.0);  // Max 3 secondes d'avance
source.register(Box::new(timer));
```

Le TimerNode :
1. Calcule le temps écoulé depuis TopZeroSync
2. Compare avec le timestamp de l'AudioSegment
3. Si trop en avance, attend (`tokio::time::sleep`)
4. Laisse passer le segment

### Pacing HTTP dans les sinks de streaming

Les sinks HTTP (StreamingFlacSink, StreamingOggFlacSink) ont leur propre pacing :

```rust
const BROADCAST_MAX_LEAD_TIME: f64 = 0.5;  // 500ms max d'avance
```

La broadcaster task :
1. Reçoit les blocs FLAC avec timestamps
2. Compare le timestamp audio avec le temps réel
3. Si > 500ms d'avance, dort pour rattraper
4. Broadcast le bloc aux clients

Cette architecture à deux niveaux (TimerNode + HTTP pacing) garantit :
- Latence faible (~500ms)
- Métadonnées synchronisées
- Pas de saturation réseau

## Gestion des erreurs

Le type `AudioError` couvre toutes les erreurs possibles :

```rust
pub enum AudioError {
    SendError,                  // Échec d'envoi MPSC
    ReceiveError,               // Échec de réception MPSC
    ProcessingError(String),    // Erreur de traitement
    TypeMismatch(TypeMismatch), // Types incompatibles
    ChildFinished,              // Enfant terminé prématurément
    ChildDied,                  // Enfant mort (channel fermé)
    IoError(String),            // Erreur I/O
}
```

Les erreurs se propagent via `Result<(), AudioError>` à travers le pipeline.

## Exemples de pipelines complets

### Pipeline de lecture de fichier

```rust
use pmoaudio::{FileSource, ToF32Node, AudioSink};

let mut source = FileSource::new("input.flac");
let converter = ToF32Node::new();
let (sink, rx) = AudioSink::new();

source.register(converter);
converter.register(Box::new(sink));

let handle = source.start();
handle.wait().await?;
```

### Pipeline de streaming HTTP

```rust
use pmoaudio::{HttpSource, TimerNode};
use pmoaudio_ext::sinks::StreamingFlacSink;

let mut http_source = HttpSource::new("https://...");
let mut timer = TimerNode::new(3.0);
let (flac_sink, stream_handle) = StreamingFlacSink::new(
    EncoderOptions::default(),
    16,
);

http_source.register(Box::new(timer));
timer.register(Box::new(flac_sink));

let pipeline = http_source.start();

// Servir via HTTP
let stream = stream_handle.subscribe_flac();
// ... envoyer au client HTTP
```

### Pipeline avec cache et conversion

```rust
use pmoaudio::{HttpSource, ToI32Node};
use pmoaudio_ext::sinks::FlacCacheSink;

let mut source = HttpSource::new("https://...");
let converter = ToI32Node::new();
let (cache_sink, rx) = FlacCacheSink::new(cache, cover_cache);

source.register(converter);
converter.register(Box::new(cache_sink));

let handle = source.start();
```

## Performances

### Latence typique

- **Chunk duration** : 50ms par défaut (`DEFAULT_CHUNK_DURATION_MS`)
- **Canal buffering** : 16 chunks = ~800ms de buffer
- **TimerNode** : 3 secondes de lead time max
- **HTTP pacing** : 500ms de lead time max
- **Latence totale** : ~1-2 secondes end-to-end

### Optimisations SIMD

Les conversions et opérations DSP utilisent SIMD quand disponible :

- **ARM NEON** : ~4-8 échantillons/cycle
- **x86_64 AVX2** : ~8-16 échantillons/cycle
- **Fallback scalaire** : ~1-2 échantillons/cycle

### Throughput

Sur un CPU moderne (2023) :
- **Décodage FLAC** : ~200-400x temps réel
- **Encodage FLAC** : ~50-100x temps réel
- **Conversion de type** : ~1000x temps réel
- **Rééchantillonnage** : ~100x temps réel (quality=high)

## Conclusion

L'architecture de **pmoaudio** offre :

1. **Modularité** : Composez des pipelines complexes avec des nodes simples
2. **Performance** : Zero-Copy, SIMD, async/await efficace
3. **Type-Safety** : Garanties à la compilation + vérifications runtime
4. **Robustesse** : Backpressure automatique, gestion d'erreurs propre
5. **Extensibilité** : Ajoutez vos propres nodes via `NodeLogic`

Pour plus de détails sur des composants spécifiques, consultez la documentation Rust (`cargo doc --open`).
