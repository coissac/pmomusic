# RadioParadiseStreamSource - Documentation Technique

## Vue d'ensemble

`RadioParadiseStreamSource` est un nœud source pour `pmoaudio` qui télécharge et décode les blocs FLAC de Radio Paradise en temps réel, avec gestion automatique des transitions entre pistes (TrackBoundary).

## Architecture

### Pattern Node<L>

Suit l'architecture séparée logique/pipeline de `pmoaudio` :

```
RadioParadiseStreamSource (wrapper)
    └── Node<RadioParadiseStreamSourceLogic>
            └── RadioParadiseStreamSourceLogic (logique métier)
```

### RadioParadiseStreamSourceLogic

Responsabilités :
- **File d'attente** : `VecDeque<EventId>` pour les blocks à télécharger
- **Cache anti-redondance** : `HashSet<EventId>` pour 10 blocs récents
- **Téléchargement** : Fetch bloc FLAC (bitrate=4 uniquement)
- **Décodage** : Stream FLAC via `pmoflac::decode_audio_stream`
- **Timing** : Calcul précis pour insertion TrackBoundary

## Flux d'exécution

```
┌─────────────────────────────────────────────────────────┐
│ 1. Attente block ID (timeout 3s)                       │
│    └─> VecDeque::pop_front()                           │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│ 2. Vérification cache                                   │
│    └─> HashSet::contains(&event_id)                    │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│ 3. Téléchargement métadonnées                          │
│    └─> client.get_block(event_id)                      │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│ 4. Téléchargement FLAC (bitrate=4)                     │
│    └─> client.download_block_file(&block, 4)           │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│ 5. Décodage streaming                                   │
│    └─> pmoflac::decode_audio_stream(reader)            │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│ 6. Découpage en chunks                                  │
│    └─> pcm_to_audio_chunk(pcm, sr, bps)                │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│ 7. Insertion TrackBoundary (timing sample-based)       │
│    └─> elapsed_ms = (total_samples * 1000) / sr        │
└─────────────────────────────────────────────────────────┘
```

## Timing TrackBoundary

### Algorithme

```rust
let elapsed_ms = (total_samples * 1000) / sample_rate as u64;

if elapsed_ms >= song.elapsed {
    // Envoyer TrackBoundary AVANT le chunk (même order)
    send_track_boundary(*order, song, block).await;
}
```

### Exemple concret

Bloc FLAC contenant 3 chansons :
- Song 0 : `elapsed = 0ms`
- Song 1 : `elapsed = 180000ms` (3min)
- Song 2 : `elapsed = 420000ms` (7min)

Timeline :
```
0ms              180000ms            420000ms
│                │                   │
Song 0           TrackBoundary       TrackBoundary
                 └─> Song 1          └─> Song 2
```

## SyncMarker Order

**Règle** : TrackBoundary a le **même order** que le chunk suivant.

```rust
// TrackBoundary order = 42
AudioSegment::new_sync(42, SyncMarker::TrackBoundary { ... })

// Chunk suivant order = 42
AudioSegment::new_audio(42, AudioChunk::I16(...))
```

## Gestion du cache

### Stratégie FIFO simple

```rust
const RECENT_BLOCKS_CACHE_SIZE: usize = 10;

fn mark_block_downloaded(&mut self, event_id: EventId) {
    self.recent_blocks.insert(event_id);

    if self.recent_blocks.len() > RECENT_BLOCKS_CACHE_SIZE {
        // Retirer un élément (ordre non garanti avec HashSet)
        if let Some(&first) = self.recent_blocks.iter().next() {
            self.recent_blocks.remove(&first);
        }
    }
}
```

## Support FLAC

### Formats supportés

- **16-bit** : `AudioChunk::I16`
- **24-bit** : `AudioChunk::I24`

### Conversion PCM

```rust
match bits_per_sample {
    16 => {
        let samples: Vec<i16> = pcm_data
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        AudioChunk::I16(...)
    }
    24 => {
        let samples: Vec<I24> = pcm_data
            .chunks_exact(3)
            .map(|chunk| {
                let value = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], 0]) >> 8;
                I24::from_i32(value)
            })
            .collect();
        AudioChunk::I24(...)
    }
}
```

## Métadonnées

### TrackMetadata

Champs extraits de `Song` :
- `title` : Titre de la chanson
- `artist` : Artiste
- `album` : Album (optionnel)
- `year` : Année (optionnel)
- `cover_url` : URL de la pochette (async via tokio::spawn)

### Gestion asynchrone du cover

```rust
tokio::spawn(async move {
    if let Ok(mut meta) = metadata_clone.write().await {
        let _ = meta.set_cover_url(Some(cover_url)).await;
    }
});
```

## API Publique

### Création

```rust
pub fn new(client: RadioParadiseClient, chunk_duration_ms: u32) -> Self
```

### Configuration

```rust
pub fn push_block_id(&mut self, event_id: EventId)
```

Ajoute un block ID à télécharger dans la file d'attente.

### Exécution

```rust
async fn run(self: Box<Self>, stop_token: CancellationToken) -> Result<(), AudioError>
```

Hérite de `AudioPipelineNode`.

## Exemple d'utilisation

Voir `examples/radio_paradise_stream.rs` pour :
- Utilisation basique
- Intégration avec nowplaying stream
- Connexion à un sink

## Constantes

```rust
const BLOCK_ID_TIMEOUT_SECS: u64 = 3;           // Timeout attente nouveau block
const RECENT_BLOCKS_CACHE_SIZE: usize = 10;     // Taille cache anti-redondance
```

## Dépendances

- `pmoaudio` : Pipeline audio, types AudioChunk/AudioSegment
- `pmoflac` : Décodage FLAC streaming
- `pmometadata` : Métadonnées pistes
- `futures-util` : StreamExt pour le décodage
- `tokio` : Runtime async
- `tokio-util` : StreamReader, CancellationToken

## Feature gate

```toml
[features]
pmoaudio = ["dep:pmoaudio", "dep:pmoflac", "dep:pmometadata", "dep:futures-util"]
```

Activer avec : `cargo build -p pmoparadise --features pmoaudio`
