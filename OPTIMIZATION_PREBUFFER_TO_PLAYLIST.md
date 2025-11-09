# Optimisation: Réduction du délai prebuffer → playlist (19s → 1s)

## Contexte

Le système de progressive caching fonctionne correctement, mais il y a un délai non optimal entre le moment où le prebuffer est atteint et le moment où la track est ajoutée à la playlist.

### État actuel (branche `claude/fix-play-and-cache-streaming-011CUsMBxH4fsgoadgkiPdoK`)

**Timing mesuré:**
```
t=0.6s   : Prebuffer complete (512KB téléchargés) ✅
t=19.2s  : tokio::join!() complete (pump_future finit)
t=19.2s  : Track added to playlist
t=19.7s  : Playback starts
```

**Délai total: ~19 secondes**

### Code actuel problématique

Location: `pmoaudio-ext/src/sinks/flac_cache_sink.rs:167-178`

```rust
// Exécuter pump et add_from_reader en parallèle
let pump_future = pump_track_segments(
    first_segment,
    &mut rx,      // ← emprunte muablement rx
    pcm_tx,
    bits_per_sample,
    sample_rate,
    &stop_token,
);

// Attendre les deux tâches en parallèle
let (cache_result, pump_result) = tokio::join!(cache_future, pump_future);

let pk = cache_result.map_err(|e| {
    AudioError::ProcessingError(format!("Failed to add to cache: {}", e))
})?;

let (_chunks, _samples, _duration_sec, stop_reason) = pump_result?;
```

**Le problème:** `tokio::join!()` attend que **LES DEUX** futures se terminent:
- `cache_future` retourne après prebuffer (~0.6s) ✅
- `pump_future` lit **toute** la première track du RadioParadiseStreamSource (~19s) ⏱️

Donc même si le prebuffer est atteint en 0.6s, on attend 19s avant de push à la playlist!

## Objectif

Réduire le délai à **~1 seconde** en pushant à la playlist **immédiatement après le prebuffer**, sans attendre que `pump_future` se termine.

**Timing visé:**
```
t=0.6s   : Prebuffer complete ✅
t=0.7s   : Track added to playlist ← IMMÉDIAT!
t=1.2s   : Playback starts ← ~1 seconde!
t=19.2s  : pump_future finit en arrière-plan
```

## Contraintes techniques

### 1. Problème du borrow checker

`pump_future` emprunte muablement `rx`:
```rust
async fn pump_track_segments(
    first_segment: Arc<AudioSegment>,
    rx: &mut mpsc::Receiver<Arc<AudioSegment>>,  // ← &mut borrow
    // ...
)
```

On ne peut pas faire:
```rust
tokio::pin!(cache_future);
tokio::pin!(pump_future);  // ← pump_future contient un &mut rx

let pk = cache_future.await;  // cache_future termine

// ❌ ERREUR: on a toujours un borrow mutable de rx dans pump_future
// On ne peut pas continuer à utiliser rx (ou l'objet qui le contient)
playlist_handle.push(pk.clone()).await;

let result = pump_future.await;  // pump_future continue
```

Le borrow checker nous empêche d'attendre `cache_future` seul, puis de faire d'autres opérations, puis d'attendre `pump_future`, car `pump_future` garde un borrow mutable de `rx` pendant toute sa durée de vie.

### 2. Contraintes de l'API

- `pump_track_segments()` doit lire `rx` pour recevoir les segments du RadioParadiseStreamSource
- Le FlacCacheSinkLogic doit garder ownership de `rx` pour traiter les tracks suivantes
- `pump_future` ne peut pas être spawné dans un tokio::spawn car il retourne un `StopReason` nécessaire pour la logique métier

## Solutions possibles

### Solution A: Refactoriser pump_track_segments pour prendre ownership de rx

**Approche:**
1. Créer `pump_track_segments_owned` qui prend ownership de `rx`
2. Cette fonction retourne `(result, rx)` - elle rend ownership de `rx`
3. Spawner cette future dans tokio::spawn
4. Attendre cache_future seul, push immédiatement
5. Attendre la task spawnée plus tard

**Signature:**
```rust
async fn pump_track_segments_owned(
    first_segment: Arc<AudioSegment>,
    rx: mpsc::Receiver<Arc<AudioSegment>>,  // ownership!
    pcm_tx: mpsc::Sender<Vec<u8>>,
    bits_per_sample: u8,
    expected_rate: u32,
    stop_token: CancellationToken,
) -> Result<(u64, u64, f64, StopReason, mpsc::Receiver<Arc<AudioSegment>>), AudioError>
//                                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ rend rx
```

**Utilisation:**
```rust
let pump_handle = tokio::spawn(pump_track_segments_owned(
    first_segment,
    rx,  // move ownership
    pcm_tx,
    bits_per_sample,
    sample_rate,
    stop_token.clone(),
));

// Attendre SEULEMENT le prebuffer
let pk = cache_future.await?;

// Push IMMÉDIATEMENT à la playlist
#[cfg(feature = "playlist")]
if let Some(ref playlist_handle) = self.playlist_handle {
    playlist_handle.push(pk.clone()).await?;
}

// MAINTENANT attendre que pump finisse
let (result, rx_returned) = pump_handle.await.unwrap()?;
rx = rx_returned;  // récupérer rx pour la prochaine track
```

**Avantages:**
- ✅ Pas de problème de borrow checker
- ✅ Push immédiat après prebuffer
- ✅ Délai réduit à ~1s

**Inconvénients:**
- ⚠️ Nécessite de modifier la signature de `pump_track_segments`
- ⚠️ Plus complexe (ownership passé puis rendu)

### Solution B: Utiliser un channel pour signaler le prebuffer

**Approche:**
1. Créer un oneshot channel `(prebuffer_tx, prebuffer_rx)`
2. `cache_future` envoie le pk via `prebuffer_tx` dès le prebuffer atteint
3. Le code principal attend `prebuffer_rx`, push immédiatement
4. Puis attend `tokio::join!()` normalement

**Code:**
```rust
let (prebuffer_tx, prebuffer_rx) = tokio::sync::oneshot::channel();

let cache_future = async {
    let pk = self.cache.add_from_reader(...).await?;
    let _ = prebuffer_tx.send(pk.clone());  // Signal prebuffer!
    Ok(pk)
};

let pump_future = pump_track_segments(...);

// Spawner les deux en parallèle
let cache_handle = tokio::spawn(cache_future);
let pump_handle = tokio::spawn(pump_future);

// Attendre SEULEMENT le signal de prebuffer
let pk = prebuffer_rx.await.unwrap();

// Push IMMÉDIATEMENT à la playlist
playlist_handle.push(pk.clone()).await?;

// Puis attendre que tout finisse
let (cache_result, pump_result) = tokio::join!(cache_handle, pump_handle);
```

**Avantages:**
- ✅ Pas besoin de changer les signatures
- ✅ Push immédiat après prebuffer

**Inconvénients:**
- ⚠️ Nécessite de wrapper cache_future pour envoyer le signal
- ⚠️ Ajoute un oneshot channel

### Solution C: Modifier l'API du cache pour avoir un callback

**Approche:**
1. Ajouter un paramètre callback à `add_from_reader()`
2. Le cache appelle ce callback dès le prebuffer atteint
3. Le callback push à la playlist

**Signature:**
```rust
pub async fn add_from_reader_with_callback<R, F>(
    &self,
    source_uri: Option<&str>,
    reader: R,
    length: Option<u64>,
    collection: Option<&str>,
    on_prebuffer: F,  // ← nouveau callback
) -> Result<String>
where
    R: AsyncRead + Send + Unpin + 'static,
    F: FnOnce(String) + Send + 'static,  // F reçoit le pk
```

**Avantages:**
- ✅ API propre et réutilisable
- ✅ Pas de problème de borrow checker

**Inconvénients:**
- ⚠️ Nécessite de modifier l'API du cache (impact sur autres parties du code)
- ⚠️ Ajoute de la complexité à l'API

## Recommandation

**Je recommande la Solution A** (refactoriser `pump_track_segments_owned`):
- Plus explicite et claire
- Pas d'impact sur l'API du cache
- Ownership bien défini (passage puis retour de rx)
- Testable indépendamment

## Plan d'implémentation

### Étape 1: Créer pump_track_segments_owned

Location: `pmoaudio-ext/src/sinks/flac_cache_sink.rs`

```rust
/// Pompe les segments pour une seule track (s'arrête au TrackBoundary).
///
/// Version qui prend ownership de rx pour permettre un await séparé du cache.
/// Retourne rx à la fin pour permettre le traitement des tracks suivantes.
async fn pump_track_segments_owned(
    first_segment: Arc<AudioSegment>,
    mut rx: mpsc::Receiver<Arc<AudioSegment>>,
    pcm_tx: mpsc::Sender<Vec<u8>>,
    bits_per_sample: u8,
    expected_rate: u32,
    stop_token: CancellationToken,
) -> Result<(u64, u64, f64, StopReason, mpsc::Receiver<Arc<AudioSegment>>), AudioError> {
    let mut chunks = 0u64;
    let mut samples = 0u64;
    let mut duration_sec = 0.0f64;

    // Traiter le premier segment
    if let Some(chunk) = first_segment.as_chunk() {
        let pcm_bytes = chunk_to_pcm_bytes(chunk, bits_per_sample)?;
        if !pcm_bytes.is_empty() {
            if pcm_tx.send(pcm_bytes).await.is_err() {
                drop(pcm_tx);
                return Ok((chunks, samples, duration_sec, StopReason::ChannelClosed, rx));
            }
            chunks += 1;
            samples += chunk.len() as u64;
            duration_sec += chunk.len() as f64 / expected_rate as f64;
        }
    }

    // Loop pour le reste des segments...
    loop {
        let segment = tokio::select! {
            result = rx.recv() => {
                match result {
                    Some(seg) => seg,
                    None => {
                        drop(pcm_tx);
                        return Ok((chunks, samples, duration_sec, StopReason::ChannelClosed, rx));
                    }
                }
            }
            _ = stop_token.cancelled() => {
                drop(pcm_tx);
                return Ok((chunks, samples, duration_sec, StopReason::ChannelClosed, rx));
            }
        };

        match &segment.segment {
            _AudioSegment::Chunk(chunk) => {
                let pcm_bytes = chunk_to_pcm_bytes(chunk, bits_per_sample)?;
                if !pcm_bytes.is_empty() {
                    if pcm_tx.send(pcm_bytes).await.is_err() {
                        drop(pcm_tx);
                        return Ok((chunks, samples, duration_sec, StopReason::ChannelClosed, rx));
                    }
                    chunks += 1;
                    samples += chunk.len() as u64;
                    duration_sec += chunk.len() as f64 / expected_rate as f64;
                }
            }
            _AudioSegment::Sync(marker) => match &**marker {
                SyncMarker::TrackBoundary { metadata, .. } => {
                    drop(pcm_tx);
                    return Ok((chunks, samples, duration_sec, StopReason::TrackBoundary(metadata.clone()), rx));
                }
                SyncMarker::EndOfStream => {
                    drop(pcm_tx);
                    return Ok((chunks, samples, duration_sec, StopReason::EndOfStream, rx));
                }
                _ => continue,
            },
        }
    }
}
```

### Étape 2: Modifier FlacCacheSinkLogic::process

Location: `pmoaudio-ext/src/sinks/flac_cache_sink.rs:~167`

```rust
let collection_ref = self.collection.as_deref();
let cache_future = self.cache.add_from_reader(
    None,
    flac_stream,
    None,
    collection_ref,
);

// Spawner pump_future avec ownership de rx
let pump_handle = tokio::spawn(pump_track_segments_owned(
    first_segment,
    rx,  // move ownership!
    pcm_tx,
    bits_per_sample,
    sample_rate,
    stop_token.clone(),
));

// Attendre SEULEMENT le prebuffer (cache retourne après 512KB)
tracing::debug!("FlacCacheSink: Waiting for cache prebuffer to complete");
let pk = cache_future.await.map_err(|e| {
    AudioError::ProcessingError(format!("Failed to add to cache: {}", e))
})?;

tracing::debug!("FlacCacheSink: Prebuffer complete with pk {}, pushing to playlist NOW", pk);

// Copier les métadonnées AVANT push
if let Some(src_metadata) = track_metadata.clone() {
    let dest_metadata = self.cache.track_metadata(&pk);
    pmometadata::copy_metadata_into(&src_metadata, &dest_metadata)
        .await
        .map_err(|e| {
            AudioError::ProcessingError(format!("Failed to copy metadata to cache: {}", e))
        })?;
}

// Push IMMÉDIATEMENT à la playlist (après prebuffer, avant pump complet!)
#[cfg(feature = "playlist")]
if let Some(ref playlist_handle) = self.playlist_handle {
    tracing::debug!("FlacCacheSink: Pushing pk {} to playlist", pk);
    playlist_handle.push(pk.clone()).await.map_err(|e| {
        AudioError::ProcessingError(format!("Failed to add to playlist: {}", e))
    })?;
    tracing::debug!("FlacCacheSink: Successfully pushed to playlist");
}

// MAINTENANT attendre que pump finisse (il continue en arrière-plan)
tracing::debug!("FlacCacheSink: Waiting for pump to complete");
let pump_result = pump_handle.await.map_err(|e| {
    AudioError::ProcessingError(format!("Pump task panicked: {}", e))
})?;

let (_chunks, _samples, _duration_sec, stop_reason, rx_returned) = pump_result?;
rx = rx_returned;  // récupérer rx pour la prochaine track
tracing::debug!("FlacCacheSink: Pump completed");

// Continuer avec download des covers en arrière-plan...
```

### Étape 3: Tester

```bash
# Nettoyer et rebuild
rm -rf /tmp/pmomusic_test
source setup-env.sh
cargo build --example play_and_cache --features full

# Tester avec logs de timing
RUST_LOG=debug target/debug/examples/play_and_cache 0 --null-audio 2>&1 | \
  grep -E "Prebuffer complete|Pushing pk.*to playlist|popped track" | \
  head -20
```

**Résultats attendus:**
```
[TIME_A] FlacCacheSink: Prebuffer complete with pk XXX, pushing to playlist NOW
[TIME_B] FlacCacheSink: Successfully pushed to playlist
[TIME_C] PlaylistSourceLogic: popped track from playlist

Délai (TIME_C - TIME_A) devrait être < 1 seconde!
```

### Étape 4: Valider le comportement

Vérifier que:
1. ✅ Le prebuffer est atteint rapidement (~0.6s)
2. ✅ Le push à la playlist est immédiat (~0.1s après prebuffer)
3. ✅ La lecture démarre rapidement (~1s total)
4. ✅ Toutes les tracks se suivent correctement
5. ✅ Les completion markers sont créés
6. ✅ Les tracks suivantes fonctionnent (rx est bien récupéré)
7. ✅ Pas de panic ou deadlock

## Debugging

### Si le borrow checker proteste

Vérifier que:
- `pump_track_segments_owned` prend bien ownership de `rx` (pas `&mut`)
- `rx` est bien retourné dans le tuple de retour
- `rx = rx_returned;` récupère bien ownership après await

### Si les tracks suivantes ne fonctionnent pas

Vérifier que:
- `rx` est bien réassigné après le pump: `rx = rx_returned;`
- La loop dans `process()` continue correctement avec le nouveau `rx`

### Si le timing n'est pas amélioré

Ajouter des logs avec timestamps:
```rust
let start = std::time::Instant::now();
let pk = cache_future.await?;
tracing::info!("Prebuffer took {:?}", start.elapsed());

let start2 = std::time::Instant::now();
playlist_handle.push(pk.clone()).await?;
tracing::info!("Playlist push took {:?}", start2.elapsed());
```

## Fichiers à modifier

1. **pmoaudio-ext/src/sinks/flac_cache_sink.rs**
   - Ajouter `pump_track_segments_owned()` (~ligne 432)
   - Modifier `FlacCacheSinkLogic::process()` (~ligne 167)

## Tests de régression

Après l'implémentation, tester:

```bash
# Test 1: Premier download (cache vide)
rm -rf /tmp/pmomusic_test
target/debug/examples/play_and_cache 0 --null-audio

# Test 2: Deuxième download (fichier déjà en cache)
# Ne pas supprimer /tmp/pmomusic_test
target/debug/examples/play_and_cache 0 --null-audio

# Test 3: Download interrompu (Ctrl+C)
target/debug/examples/play_and_cache 0 --null-audio
# Appuyer Ctrl+C après 2 secondes

# Test 4: Plusieurs tracks consécutives
# Laisser tourner 1 minute pour voir plusieurs tracks
timeout 60 target/debug/examples/play_and_cache 0 --null-audio
```

## Métriques de succès

- ✅ Délai prebuffer → playlist: **< 1 seconde** (actuellement ~19s)
- ✅ Délai prebuffer → lecture: **< 2 secondes** (actuellement ~19.5s)
- ✅ Pas de régression fonctionnelle
- ✅ Toutes les tracks se suivent correctement
- ✅ Les completion markers sont créés

## Références

- Branche actuelle: `claude/fix-play-and-cache-streaming-011CUsMBxH4fsgoadgkiPdoK`
- Code de référence: commit `ed0bbfb` (Add FlacCacheSink debug logs - system now works!)
- Issue originale: "play_and_cache n'a pas le comportement souhaité"
