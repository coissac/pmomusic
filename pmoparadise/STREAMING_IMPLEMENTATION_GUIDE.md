# Guide d'Implémentation - Streaming Progressif avec Claxon

## Objectif
Transformer le worker pour qu'il décode le FLAC en streaming au fur et à mesure du téléchargement HTTP, afin d'envoyer le premier morceau au cache en **~6-8 secondes** au lieu de 12-16 secondes.

---

## Vue d'Ensemble de l'Architecture

### Architecture Actuelle (LENTE - 12-16s)
```
HTTP Request → Télécharger TOUT le block (75-100 MB) → Symphonia (Cursor)
    ↓
Décoder TOUT en PCM
    ↓
Pour chaque morceau:
    - Découper PCM
    - Encoder FLAC
    - Envoyer au cache
```

**Problème** : On attend le téléchargement complet avant de commencer quoi que ce soit.

### Architecture Cible (RAPIDE - 6-8s)
```
HTTP Stream → StreamReader (adapt async → sync)
    ↓
claxon::FlacReader (lit frame par frame SANS Seek)
    ↓
Accumule PCM dans buffer
    ↓
Dès que buffer.samples >= durée_morceau_1:
    - Découper buffer
    - Encoder FLAC
    - Envoyer au cache (morceau 1 disponible!)
    ↓
Continue streaming pour morceaux 2, 3, ...
```

---

## Étape 1 : Créer AsyncReadAdapter

### But
Convertir `Stream<Item = Result<Bytes>>` (async) en `impl Read` (sync) pour claxon.

### Localisation
Ajouter au début de `paradise/worker.rs`, après les imports.

### Code Complet

```rust
use std::pin::Pin;
use std::task::{Context, Poll};
use futures::stream::Stream;
use std::io::{self, Read};
use std::collections::VecDeque;
use tokio::runtime::Handle;
use bytes::Bytes;

/// Adapte un Stream async en impl Read synchrone
///
/// Utilise le runtime tokio courant pour bloquer sur le stream async.
/// ATTENTION: Doit être appelé depuis un contexte tokio (spawn_blocking).
struct AsyncReadAdapter {
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    buffer: VecDeque<u8>,
    runtime: Handle,
    done: bool,
}

impl AsyncReadAdapter {
    fn new(stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>) -> Self {
        Self {
            stream,
            buffer: VecDeque::new(),
            runtime: Handle::current(),
            done: false,
        }
    }

    fn fill_buffer(&mut self) -> io::Result<()> {
        if self.done {
            return Ok(());
        }

        // Bloquer pour récupérer le prochain chunk du stream
        let next_chunk = self.runtime.block_on(async {
            use futures::StreamExt;
            self.stream.next().await
        });

        match next_chunk {
            Some(Ok(bytes)) => {
                self.buffer.extend(bytes.iter());
                Ok(())
            }
            Some(Err(e)) => {
                self.done = true;
                Err(io::Error::new(io::ErrorKind::Other, e))
            }
            None => {
                self.done = true;
                Ok(())
            }
        }
    }
}

impl Read for AsyncReadAdapter {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Si buffer vide et stream pas terminé, remplir
        while self.buffer.is_empty() && !self.done {
            self.fill_buffer()?;
        }

        // Copier du buffer vers buf
        let to_read = buf.len().min(self.buffer.len());
        for i in 0..to_read {
            buf[i] = self.buffer.pop_front().unwrap();
        }

        Ok(to_read)
    }
}
```

### Pièges à Éviter

1. **Context Tokio** : `AsyncReadAdapter` DOIT être créé dans un contexte tokio (utilisez `tokio::task::spawn_blocking`)
2. **Deadlock** : Ne jamais appeler depuis le même thread qui exécute le stream
3. **Buffer Size** : VecDeque peut grossir - surveiller la mémoire

---

## Étape 2 : Remplacer process_block()

### Architecture de la Nouvelle Fonction

```rust
async fn process_block(&mut self, block: Block) -> Result<()> {
    // 1. Vérifications habituelles
    if self.is_recent_block(block.event) { ... }

    // 2. Lancer le streaming HTTP
    let block_url = Url::parse(&block.url)?;
    let http_stream = self.client.stream_block(&block_url).await?;

    // 3. Spawn un thread bloquant pour le décodage
    let songs_ordered = block.songs_ordered();
    let channel_id = self.descriptor.id;
    let sample_rate = 44100; // Sera détecté par claxon

    let tracks = tokio::task::spawn_blocking(move || {
        decode_and_split_streaming(
            http_stream,
            songs_ordered,
            sample_rate
        )
    }).await??;

    // 4. Pour chaque track décodé, envoyer au cache
    for (track_pcm, song_index, song) in tracks {
        let entry = self.process_song_from_pcm(
            &block,
            song_index,
            song,
            track_pcm
        ).await?;

        self.playlist.push_active(entry).await;
    }

    // 5. Mise à jour
    self.record_processed_block(block.event);
    self.next_block_hint = Some(block.end_event);
    Ok(())
}
```

---

## Étape 3 : Fonction de Décodage Streaming

### Pseudo-Code Détaillé

```rust
fn decode_and_split_streaming(
    http_stream: BlockStream, // Le stream de client.stream_block()
    songs: Vec<(usize, &Song)>,
    expected_sample_rate: u32,
) -> Result<Vec<(Vec<i32>, usize, Song)>> {
    // 1. Convertir BlockStream en AsyncReadAdapter
    let adapter = AsyncReadAdapter::new(http_stream.into_inner());
    let buffered = std::io::BufReader::new(adapter);

    // 2. Créer le FlacReader de claxon
    let mut reader = claxon::FlacReader::new(buffered)
        .map_err(|e| anyhow!("Failed to create FLAC reader: {e}"))?;

    let streaminfo = reader.streaminfo();
    let channels = streaminfo.channels as usize;
    let sample_rate = streaminfo.sample_rate;
    let bits_per_sample = streaminfo.bits_per_sample;

    // 3. Buffer PCM accumulé
    let mut accumulated_samples: Vec<i32> = Vec::new();
    let mut current_frame = 0; // Nombre de frames PCM lues
    let mut tracks = Vec::new();
    let mut next_song_idx = 0;

    // 4. Lire frame par frame
    loop {
        // Lire une frame FLAC
        let frame = match reader.read_next_or_eof(/* buffer */) {
            Ok(Some(frame_data)) => frame_data,
            Ok(None) => break, // EOF
            Err(e) => return Err(anyhow!("FLAC decode error: {e}")),
        };

        // Convertir frame en i32 et accumuler
        // NOTE: claxon retourne des samples par canal, il faut entrelacer
        let samples_in_frame = frame.len() / channels;
        for sample_idx in 0..samples_in_frame {
            for ch in 0..channels {
                let sample = frame[ch * samples_in_frame + sample_idx];
                // Normaliser selon bits_per_sample
                let normalized = normalize_sample(sample, bits_per_sample);
                accumulated_samples.push(normalized);
            }
        }

        current_frame += samples_in_frame;

        // 5. Vérifier si on a atteint la fin du morceau courant
        if next_song_idx < songs.len() {
            let (song_index, song) = &songs[next_song_idx];
            let song_end_frame = if next_song_idx + 1 < songs.len() {
                // Fin = début du prochain morceau
                ms_to_frames(songs[next_song_idx + 1].1.elapsed, sample_rate)
            } else {
                // Dernier morceau = fin du block
                usize::MAX // On prendra tout jusqu'à la fin
            };

            if current_frame >= song_end_frame {
                // 6. Découper le buffer
                let song_start_frame = ms_to_frames(song.elapsed, sample_rate);
                let start_sample = song_start_frame * channels;
                let end_sample = song_end_frame * channels;

                let track_samples = accumulated_samples[start_sample..end_sample.min(accumulated_samples.len())]
                    .to_vec();

                tracks.push((track_samples, *song_index, (*song).clone()));

                next_song_idx += 1;

                // IMPORTANT: Premier morceau envoyé ici!
                // Les suivants continueront pendant que le premier est traité
            }
        }
    }

    // 7. Traiter le dernier morceau si nécessaire
    if next_song_idx < songs.len() {
        let (song_index, song) = &songs[next_song_idx];
        let song_start_frame = ms_to_frames(song.elapsed, sample_rate);
        let start_sample = song_start_frame * channels;
        let track_samples = accumulated_samples[start_sample..].to_vec();
        tracks.push((track_samples, *song_index, (*song).clone()));
    }

    Ok(tracks)
}

fn normalize_sample(sample: i32, bits_per_sample: u32) -> i32 {
    match bits_per_sample {
        0..=16 => sample << 16,  // Shift to 32-bit range
        17..=24 => sample << 8,
        _ => sample,
    }
}

fn ms_to_frames(ms: u64, sample_rate: u32) -> usize {
    ((ms as u128 * sample_rate as u128) / 1000) as usize
}
```

---

## Étape 4 : Adapter process_song

### Nouvelle Signature

```rust
async fn process_song_from_pcm(
    &self,
    block: &Block,
    song_index: usize,
    song: &Song,
    track_samples: Vec<i32>, // PCM déjà découpé
) -> Result<Arc<PlaylistEntry>>
```

### Changements

1. **Supprimer** le découpage (déjà fait dans decode_and_split_streaming)
2. **Garder** l'encodage FLAC
3. **Garder** le cache audio/cover
4. **Garder** la création de PlaylistEntry

```rust
async fn process_song_from_pcm(
    &self,
    block: &Block,
    song_index: usize,
    song: &Song,
    track_samples: Vec<i32>,
) -> Result<Arc<PlaylistEntry>> {
    // 1. Encoder PCM → FLAC (déjà existant)
    let flac_bytes = encode_samples_to_flac(
        track_samples,
        2, // channels - TODO: passer en paramètre
        44100, // sample_rate - TODO: passer en paramètre
        16, // bits - TODO: passer en paramètre
    ).await?;

    // 2. Calculer track_id
    let track_id = self.compute_track_id(&flac_bytes);
    let placeholder_uri = format!("{}#{}", block.url, song_index);

    // 3. Cache cover (inchangé)
    let cover_pk = self.cache_cover(block, song).await?;

    // 4. Cache audio (inchangé)
    let flac_len = flac_bytes.len() as u64;
    let reader = StreamReader::new(stream::iter(vec![Ok::<_, std::io::Error>(
        Bytes::from(flac_bytes)
    )]));
    let audio_pk = self.cache_manager
        .cache_audio_from_reader(&track_id, reader, Some(flac_len))
        .await?;

    // 5. Metadata (inchangé)
    let metadata = TrackMetadata {
        original_uri: placeholder_uri,
        cached_audio_pk: Some(audio_pk.clone()),
        cached_cover_pk: cover_pk,
    };
    self.cache_manager.update_metadata(track_id.clone(), metadata).await;

    // 6. Créer PlaylistEntry (inchangé)
    let duration_ms = song.duration;
    let file_path = self.cache_manager.audio_file_path(&audio_pk).await;
    let entry = Arc::new(PlaylistEntry::new(
        track_id,
        self.descriptor.id,
        Arc::new(song.clone()),
        Utc::now(),
        duration_ms,
        Some(audio_pk),
        file_path,
        self.active_clients,
    ));

    Ok(entry)
}
```

---

## Étape 5 : API de claxon

### Documentation Claxon

```rust
// Créer un reader
let mut reader = claxon::FlacReader::new(buffered_reader)?;

// Obtenir les infos du stream
let info = reader.streaminfo();
// info.channels: u32
// info.sample_rate: u32
// info.bits_per_sample: u32
// info.samples: Option<u64> (peut être None pour streams)

// Lire des samples
// Option 1: Frame par frame (recommandé pour streaming)
let mut samples = vec![0i32; info.channels as usize * 4096];
loop {
    match reader.read_next_or_eof(samples.as_mut_slice()) {
        Ok(Some(n)) => {
            // n samples lus, entrelacer si nécessaire
        }
        Ok(None) => break, // EOF
        Err(e) => return Err(e),
    }
}

// Option 2: Iterator (plus simple mais moins contrôle)
for sample in reader.samples() {
    let s = sample?;
    // Traiter sample par sample
}
```

### Entrelacement des Samples

Claxon retourne les samples **par canal** :
```
Buffer claxon: [L0, L1, L2, ..., Ln, R0, R1, R2, ..., Rn]
```

Il faut entrelacer pour PCM standard :
```
Buffer PCM:    [L0, R0, L1, R1, L2, R2, ..., Ln, Rn]
```

```rust
fn interleave_samples(frame: &[i32], channels: usize) -> Vec<i32> {
    let samples_per_channel = frame.len() / channels;
    let mut interleaved = Vec::with_capacity(frame.len());

    for i in 0..samples_per_channel {
        for ch in 0..channels {
            interleaved.push(frame[ch * samples_per_channel + i]);
        }
    }

    interleaved
}
```

---

## Étape 6 : Gestion d'Erreurs

### Erreurs Potentielles

1. **Stream HTTP interrompu** : Gérer les EOF prématurés
2. **Mauvais timing** : Vérifier que `song.elapsed` < durée totale
3. **Corruption FLAC** : claxon peut échouer sur frames corrompues

### Pattern de Gestion

```rust
match reader.read_next_or_eof(buffer) {
    Ok(Some(n)) => {
        // Traiter n samples
    }
    Ok(None) => {
        // EOF normal
        break;
    }
    Err(claxon::Error::FormatError(msg)) => {
        // Frame corrompue, continuer ou abandonner?
        warn!("FLAC format error: {}", msg);
        continue; // Ou break selon la criticité
    }
    Err(e) => {
        // Erreur fatale
        return Err(anyhow!("FLAC decode error: {}", e));
    }
}
```

---

## Étape 7 : Tests Recommandés

### Test 1 : AsyncReadAdapter

```rust
#[tokio::test]
async fn test_async_read_adapter() {
    let data = vec![
        Ok(Bytes::from_static(b"Hello ")),
        Ok(Bytes::from_static(b"World")),
    ];
    let stream = futures::stream::iter(data);

    let mut adapter = AsyncReadAdapter::new(Box::pin(stream));
    let mut buf = [0u8; 11];
    let n = adapter.read(&mut buf).unwrap();

    assert_eq!(n, 11);
    assert_eq!(&buf, b"Hello World");
}
```

### Test 2 : Décodage d'un Petit FLAC

Créer un fichier FLAC de test (1 morceau, 10 secondes) et vérifier :
1. Le stream est lu progressivement
2. Le morceau est correctement découpé
3. Le FLAC réencodé est valide

### Test 3 : Integration Complète

1. Télécharger un vrai block Radio Paradise
2. Chronométrer le temps jusqu'au premier morceau disponible
3. Vérifier que les morceaux suivants arrivent bien

---

## Étape 8 : Optimisations Futures

### Buffer Size Tuning

```rust
// Ajuster selon le réseau
const STREAM_BUFFER_SIZE: usize = 64 * 1024; // 64 KB
```

### Parallélisation

Une fois le premier morceau envoyé, les suivants peuvent être traités en parallèle :

```rust
let mut tasks = Vec::new();
for (track_pcm, song_index, song) in tracks {
    let task = tokio::spawn(async move {
        // Encoder + envoyer au cache
    });
    tasks.push(task);
}

// Attendre tous en parallèle
futures::future::join_all(tasks).await;
```

---

## Pièges Critiques à Éviter

### 1. Seek dans claxon
**ERREUR** : claxon::FlacReader n'a **PAS** de méthode `seek()` !
- Ne tentez pas `reader.seek_to(position)` (compile pas)
- Le streaming est **séquentiel uniquement**

### 2. Thread Blocking
**ERREUR** : Créer AsyncReadAdapter dans un contexte async
```rust
// ❌ MAUVAIS
async fn foo() {
    let adapter = AsyncReadAdapter::new(stream); // Deadlock!
}

// ✅ BON
tokio::task::spawn_blocking(move || {
    let adapter = AsyncReadAdapter::new(stream);
    // ...
})
```

### 3. Normalisation des Samples
**ERREUR** : Ne pas normaliser selon bits_per_sample
- claxon retourne des samples **natifs** (16-bit → i32 avec shift)
- flacenc attend des samples dans la plage correcte
- **Toujours normaliser** selon les bits réels

### 4. Accumulation Mémoire
**ATTENTION** : `accumulated_samples` peut devenir ÉNORME (100 MB+)
- **Solution** : Ne garder que le nécessaire, supprimer les samples déjà traités
- Ou: Traiter morceau par morceau sans accumuler tout le block

---

## Mesures de Performance Attendues

### Avant (Architecture Actuelle)
- Téléchargement block : 12-16 secondes (75-100 MB @ 50 Mbps)
- Premier morceau disponible : **12-16 secondes**

### Après (Streaming Progressif)
- Temps pour 1er morceau (3 min, ~30 MB) : **~6-8 secondes**
- Amélioration : **2x plus rapide** ⚡

### Métriques à Surveiller
1. Temps entre `get_block()` et premier `push_active()`
2. Débit du stream HTTP (surveiller throttling)
3. Utilisation mémoire de `accumulated_samples`

---

## Checklist d'Implémentation

- [ ] Créer `AsyncReadAdapter` avec tests unitaires
- [ ] Remplacer `decode_block_audio()` par `decode_and_split_streaming()`
- [ ] Adapter `process_block()` pour utiliser streaming
- [ ] Créer `process_song_from_pcm()`
- [ ] Tester avec un petit FLAC local
- [ ] Tester avec un vrai block Radio Paradise
- [ ] Mesurer les performances (avant/après)
- [ ] Vérifier pas de régression sur la qualité audio
- [ ] Vérifier pas de fuite mémoire
- [ ] Ajouter logs de debug pour troubleshooting

---

## Ressources

- **claxon docs** : https://docs.rs/claxon/latest/claxon/
- **Radio Paradise API** : https://api.radioparadise.com/api
- **FLAC spec** : https://xiph.org/flac/format.html

---

Bon courage pour l'implémentation ! 🚀
