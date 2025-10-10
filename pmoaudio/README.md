# PMOAudio

Pipeline audio stéréo async optimisé pour Rust, utilisant Tokio.

## Caractéristiques

- **Pipeline push-based async** : Tous les nodes utilisent Tokio pour un traitement non-bloquant
- **Zero-copy optimisé** : Les données audio sont partagées via `Arc<Vec<f32>>` pour éviter les clonages inutiles
- **Support multiroom** : BufferNode avec buffer circulaire et offsets indépendants par abonné
- **TimerNode** : Calcul de position temporelle en temps réel
- **Backpressure** : Channels bounded avec `try_send` pour éviter les blocages

## Architecture

### AudioChunk

Structure de données pour un chunk audio stéréo :

```rust
pub struct AudioChunk {
    pub order: u64,                  // Numéro d'ordre
    pub left: Arc<Vec<f32>>,         // Canal gauche (partagé)
    pub right: Arc<Vec<f32>>,        // Canal droit (partagé)
    pub sample_rate: u32,            // Taux d'échantillonnage
}
```

Les données sont wrappées dans `Arc` pour permettre le partage sans copie entre plusieurs abonnés.

### Nodes

#### SingleSubscriberNode
- Un seul abonné
- Pas de clone inutile du Arc

#### MultiSubscriberNode
- Plusieurs abonnés
- Partage le même `Arc<AudioChunk>` avec tous

#### SourceNode
- Génère ou lit des chunks audio
- Version mock avec génération de sinusoïdes pour tests

#### DecoderNode
- Décode les chunks audio
- Supporte le passthrough et le resampling (mock)

#### DspNode
- Applique des transformations DSP
- Clone les données uniquement si modification nécessaire
- Exemple : gain, filtrage

#### BufferNode
- Buffer circulaire (`VecDeque<Arc<AudioChunk>>`)
- Support multiroom avec offsets indépendants
- `try_send` non-bloquant pour éviter de bloquer la source

#### TimerNode
- Node passthrough qui ne modifie pas les données
- Incrémente un compteur de samples
- Calcule la position : `position_sec = elapsed_samples / sample_rate`
- Fournit un `TimerHandle` pour monitoring

#### SinkNode
- Node terminal qui consomme les chunks
- Versions : silent, logging, stats, mock file writer

## Pipeline type

```
SourceNode → DecoderNode → DSPNode → BufferNode → TimerNode → SinkNode(s)
                                           ↓
                                    Multiroom Sinks
                                    (avec offsets)
```

## Exemples

### Pipeline simple

```rust
use pmoaudio::{SinkNode, SourceNode, TimerNode};

#[tokio::main]
async fn main() {
    let (mut timer, timer_tx) = TimerNode::new(10);
    let (sink, sink_tx) = SinkNode::new("Output".to_string(), 10);

    timer.add_subscriber(sink_tx);
    let timer_handle = timer.get_position_handle();

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

### Multiroom

```rust
let (buffer, buffer_tx) = BufferNode::new(50, 10);

let (sink1, sink1_tx) = SinkNode::new("Room 1".to_string(), 10);
let (sink2, sink2_tx) = SinkNode::new("Room 2".to_string(), 10);

buffer.add_subscriber_with_offset(sink1_tx, 0).await;  // Pas de délai
buffer.add_subscriber_with_offset(sink2_tx, 5).await;  // 5 chunks de retard
```

## Lancer les exemples

```bash
# Pipeline simple
cargo run --example simple_pipeline

# Pipeline complet avec tous les nodes
cargo run --example pipeline_demo

# Configuration multiroom
cargo run --example multiroom_demo

# Streaming avec timing réel
cargo run --example streaming_demo
```

## Tests

```bash
cargo test
```

20 tests unitaires couvrant :
- Propagation des chunks
- Calcul de position par TimerNode
- BufferNode multi-abonné avec offsets
- Arc sharing et zero-copy
- DSP avec gain et filtrage
- Resampling

## Optimisations

1. **Arc sharing** : Les `AudioChunk` sont clonés via `Arc::clone()` qui ne clone que le pointeur
2. **Copy-on-Write** : Les DSP nodes clonent les données uniquement si modification nécessaire
3. **Bounded channels** : Backpressure automatique
4. **try_send** : Non-bloquant pour BufferNode, permet de sauter des chunks si un abonné est saturé
5. **RwLock** : Pour partage concurrent du compteur TimerNode

## Dépendances

- `tokio` : Runtime async et channels
- `async-trait` : Traits async

## License

MIT
