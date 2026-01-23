** Tu dois suivre scrupuleusement les règles définies dans le fichier [@Rules.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules.md) **

** Cette tâche est une tâche de recherche et développement. Elle doit conduire à un prototype fonctionnel et/ou un rapport technique sur la faisabilité. **

# Support du streaming AAC dans pmoflac

## Contexte

Actuellement, `pmoflac` supporte le décodage streaming pour :
- ✅ MP3 (via `minimp3`)
- ✅ FLAC (via `claxon`)
- ✅ Ogg Vorbis (via `lewton`)
- ✅ Ogg Opus (via `opus`)
- ✅ WAV (parsing manuel)
- ✅ AIFF (parsing manuel)

**Manque critique** : Pas de support AAC, pourtant très utilisé pour :
- Streams radio live (Radio France, etc.)
- Podcasts
- Services de streaming musicaux
- Fichiers M4A/MP4

## Problématique

Le décodage AAC en **streaming infini** (radio live) est actuellement impossible dans `pmoflac`, ce qui force à :
- Soit faire un proxy passthrough (pas de transcodage FLAC)
- Soit utiliser une redirection 302 (pas de tracking)

Cela empêche d'avoir une expérience uniforme où toutes les sources servent du FLAC.

## Objectif

**Investiguer et prototyper** le support du décodage AAC streaming dans `pmoflac`, en s'inspirant de l'architecture existante (MP3, Ogg, etc.).

## Recherches préliminaires

### 1. Symphonia avec ReadOnlySource

[Symphonia](https://github.com/pdeljanov/Symphonia) est la bibliothèque Rust la plus complète pour le décodage audio. Elle fournit :

- **`ReadOnlySource`** : Wrapper pour sources non-seekable (streams infinis)
- **`AdtsReader`** : Format reader spécifique pour ADTS (AAC streaming)
- **`symphonia-codec-aac`** : Décodeur AAC-LC (Low Complexity)

**Points d'attention** :
- [Issue connue](https://github.com/RustAudio/rodio/issues/580) : Certains formats peuvent quand même réclamer le seek
- Nécessite de tester avec un vrai stream ADTS

### 2. Format ADTS

[ADTS](https://wiki.multimedia.cx/index.php/ADTS) (Audio Data Transport Stream) est le format AAC conçu pour le streaming :

- Auto-synchronisant : chaque frame a un header (12 bits `0xFFF`)
- Pas de container nécessaire (MP4, M4A)
- Utilisé par les radios en streaming
- Chaque frame contient ses métadonnées (sample rate, channels, etc.)

**Structure** :
```
Frame 1: [ADTS Header 7-9 bytes][AAC Data]
Frame 2: [ADTS Header 7-9 bytes][AAC Data]
...
```

### 3. Alternative : fdk-aac

[Bindings Rust pour fdk-aac](https://github.com/haileys/fdk-aac-rs) (bibliothèque Fraunhofer) :

**Avantages** :
- ✅ Décodeur de référence (qualité maximale)
- ✅ Support explicite du streaming chunk-by-chunk
- ✅ Buffer interne géré automatiquement
- ✅ Pas besoin de seek

**Inconvénients** :
- ❌ Dépendance C (libfdk-aac)
- ❌ Licence restrictive (non-commerciale pour certaines versions)
- ❌ Compilation plus complexe

## Plan d'investigation

### Round 1 : Prototype Symphonia ADTS

**Objectif** : Tester si Symphonia peut décoder un stream AAC infini avec `ReadOnlySource` + `AdtsReader`.

#### Étapes

1. **Créer un module de test** : `pmoflac/tests/aac_streaming_test.rs`

2. **Implémenter un décodeur basique** :
   ```rust
   use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
   use symphonia::default::get_probe;
   use symphonia_codec_aac::AdtsReader;
   
   async fn decode_aac_stream_test<R: AsyncRead + Unpin>(
       reader: R
   ) -> Result<Vec<u8>> {
       // Wrapper AsyncRead → Read synchrone (pattern pmoflac)
       let sync_reader = blocking_reader_from_async(reader);
       
       // ReadOnlySource pour stream infini
       let source = ReadOnlySource::new(sync_reader);
       let mss = MediaSourceStream::new(Box::new(source), Default::default());
       
       // Probe avec hint AAC/ADTS
       let mut hint = Hint::new();
       hint.with_extension("aac");
       
       let mut format = get_probe()
           .format(&hint, mss, &Default::default(), &Default::default())?;
       
       // Récupérer le track audio
       let track = format.default_track().unwrap();
       let mut decoder = symphonia::default::get_codecs()
           .make(&track.codec_params, &Default::default())?;
       
       let mut pcm_output = Vec::new();
       
       // Décoder frame par frame (boucle infinie jusqu'à disconnect)
       loop {
           match format.next_packet() {
               Ok(packet) => {
                   let decoded = decoder.decode(&packet)?;
                   // Convertir en PCM et accumuler
                   let samples = convert_to_pcm_bytes(decoded);
                   pcm_output.extend_from_slice(&samples);
               }
               Err(symphonia::core::errors::Error::IoError(e)) 
                   if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                   break; // Stream fermé
               }
               Err(e) => return Err(e.into()),
           }
       }
       
       Ok(pcm_output)
   }
   ```

3. **Tester avec un fichier AAC ADTS statique** :
   - Télécharger un échantillon AAC ADTS
   - Vérifier que le décodage fonctionne
   - Comparer PCM output avec ffmpeg

4. **Tester avec un stream Radio France live** :
   ```rust
   #[tokio::test]
   #[ignore = "Requires network"]
   async fn test_decode_radiofrance_stream() {
       let stream_url = "https://icecast.radiofrance.fr/fip-hifi.aac";
       let response = reqwest::get(stream_url).await.unwrap();
       let reader = response.bytes_stream();
       
       // Lire 10 secondes de stream
       let pcm = decode_aac_stream_test(reader).await.unwrap();
       
       assert!(!pcm.is_empty());
       // Vérifier format PCM (44.1kHz ou 48kHz, stéréo, 16-bit)
   }
   ```

#### Critères de succès Round 1

- ✅ Le décodeur accepte un `ReadOnlySource` sans erreur de seek
- ✅ Les frames ADTS sont correctement parsées
- ✅ Le décodage AAC → PCM fonctionne
- ✅ Un stream live (infini) peut être décodé sans plantage
- ✅ Le PCM output est valide (vérifiable avec `ffplay`)

#### Livrables Round 1

1. **Module de test** : `pmoflac/tests/aac_streaming_test.rs`
2. **Rapport technique** : `Blackboard/Report/Support_AAC_streaming_pmoflac.md`
   - Résultats des tests
   - Problèmes rencontrés (seek, parsing, etc.)
   - Métriques de performance (CPU, latence)
   - Comparaison qualité avec ffmpeg

---

### Round 2 : Intégration dans pmoflac (si Round 1 réussit)

**Objectif** : Intégrer le décodeur AAC dans l'architecture streaming de `pmoflac`.

#### Fichiers à créer/modifier

**1. `pmoflac/src/aac.rs`** (nouveau)

```rust
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use tokio::sync::mpsc;
use crate::{
    common::ChannelReader,
    decoder_common::{spawn_ingest_task, spawn_writer_task, DecodedStream},
    pcm::StreamInfo,
};

pub type AacDecodedStream = DecodedStream<AacError>;

#[derive(thiserror::Error, Debug)]
pub enum AacError {
    #[error("AAC decode error: {0}")]
    Decode(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Channel closed")]
    ChannelClosed,
}

/// Décoder un stream AAC/ADTS en PCM
pub async fn decode_aac_stream<R>(reader: R) -> Result<AacDecodedStream, AacError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    // Suivre le pattern existant (MP3, FLAC, etc.)
    let (ingest_tx, ingest_rx) = mpsc::channel(CHANNEL_CAPACITY);
    spawn_ingest_task(reader, ingest_tx);
    
    let (pcm_tx, pcm_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let (pcm_reader, pcm_writer) = tokio::io::duplex(DUPLEX_BUFFER_SIZE);
    let (info_tx, info_rx) = oneshot::channel::<Result<StreamInfo, AacError>>();
    
    let blocking_handle = tokio::task::spawn_blocking(move || -> Result<(), AacError> {
        let mut channel_reader = ChannelReader::<AacError>::new(ingest_rx);
        
        // ReadOnlySource pour stream infini
        let source = ReadOnlySource::new(&mut channel_reader);
        let mss = MediaSourceStream::new(Box::new(source), Default::default());
        
        // Probe AAC/ADTS
        let mut hint = Hint::new();
        hint.with_extension("aac");
        
        let mut format = get_probe()
            .format(&hint, mss, &Default::default(), &Default::default())
            .map_err(|e| AacError::Decode(e.to_string()))?;
        
        let track = format.default_track()
            .ok_or_else(|| AacError::Decode("No audio track found".into()))?;
        
        let mut decoder = get_codecs()
            .make(&track.codec_params, &Default::default())
            .map_err(|e| AacError::Decode(e.to_string()))?;
        
        // Extraire StreamInfo
        let codec_params = &track.codec_params;
        let info = StreamInfo {
            sample_rate: codec_params.sample_rate.unwrap_or(48000),
            channels: codec_params.channels.unwrap().count() as u8,
            bits_per_sample: 16, // AAC decode to 16-bit PCM
            total_samples: None, // Stream infini
            max_block_size: 0,
            min_block_size: 0,
        };
        
        if info_tx.send(Ok(info.clone())).is_err() {
            return Ok(());
        }
        
        // Boucle de décodage
        loop {
            match format.next_packet() {
                Ok(packet) => {
                    let decoded = decoder.decode(&packet)
                        .map_err(|e| AacError::Decode(e.to_string()))?;
                    
                    // Convertir AudioBufferRef → bytes PCM
                    let pcm_bytes = convert_audio_buffer_to_bytes(decoded, &info);
                    
                    if pcm_tx.blocking_send(Ok(pcm_bytes)).is_err() {
                        break; // Reader fermé
                    }
                }
                Err(symphonia::core::errors::Error::IoError(e)) 
                    if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break; // Stream terminé normalement
                }
                Err(e) => {
                    let msg = e.to_string();
                    let _ = pcm_tx.blocking_send(Err(AacError::Decode(msg.clone())));
                    return Err(AacError::Decode(msg));
                }
            }
        }
        
        Ok(())
    });
    
    let writer_handle = spawn_writer_task(pcm_rx, pcm_writer, blocking_handle, "aac-decode");
    let info = info_rx.await.map_err(|_| AacError::ChannelClosed)??;
    let reader = ManagedAsyncReader::new("aac-decode-writer", pcm_reader, writer_handle);
    
    Ok(DecodedStream::new(info, reader))
}

/// Convertir AudioBufferRef Symphonia → bytes PCM little-endian
fn convert_audio_buffer_to_bytes(
    audio_buffer: AudioBufferRef,
    info: &StreamInfo,
) -> Vec<u8> {
    // Implémenter conversion selon le type de buffer
    // (S16, S24, S32, F32, etc.) → i16 little-endian interleaved
    // ...
}
```

**2. `pmoflac/src/lib.rs`** (modifier)

```rust
pub mod aac;

pub use aac::{decode_aac_stream, AacDecodedStream, AacError};
```

**3. `pmoflac/src/autodetect.rs`** (modifier)

Ajouter la détection AAC/ADTS :

```rust
fn detect_format(bytes: &[u8]) -> Option<DetectedFormat> {
    // ... détections existantes ...
    
    // Détecter ADTS AAC (syncword 0xFFF)
    if is_adts(bytes) {
        return Some(DetectedFormat::Aac);
    }
    
    None
}

fn is_adts(bytes: &[u8]) -> bool {
    if bytes.len() < 2 {
        return false;
    }
    // ADTS syncword: 12 bits à 1 (0xFFF)
    bytes[0] == 0xFF && (bytes[1] & 0xF0) == 0xF0
}

pub enum DecodedAudioStream {
    // ... variants existants ...
    Aac(AacDecodedStream),
}
```

**4. `pmoflac/src/transcode.rs`** (modifier)

Ajouter AAC au transcodeur :

```rust
pub enum AudioCodec {
    // ... codecs existants ...
    Aac,
}

pub async fn transcode_to_flac_stream<R>(
    reader: R,
    options: TranscodeOptions,
) -> Result<TranscodeToFlac, TranscodeError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    // ... détection auto ...
    
    match decoded {
        // ... cas existants ...
        DecodedAudioStream::Aac(stream) => {
            transcode_from_decoded(AudioCodec::Aac, stream, options.encoder_options).await
        }
    }
}
```

**5. `pmoflac/Cargo.toml`** (modifier)

```toml
[dependencies]
# ... dépendances existantes ...

# AAC support
symphonia = { version = "0.5", features = ["aac", "isomp4"], optional = true }
symphonia-core = { version = "0.5", optional = true }
symphonia-codec-aac = { version = "0.5", optional = true }

[features]
default = ["mp3", "ogg", "opus", "wav", "aiff"]
aac = ["dep:symphonia", "dep:symphonia-core", "dep:symphonia-codec-aac"]
all = ["mp3", "ogg", "opus", "wav", "aiff", "aac"]
```

#### Tests Round 2

**Tests unitaires** :
```rust
#[tokio::test]
async fn test_decode_aac_to_pcm() {
    let aac_data = include_bytes!("../test-data/sample.aac");
    let stream = decode_aac_stream(&aac_data[..]).await.unwrap();
    
    let info = stream.info();
    assert_eq!(info.sample_rate, 48000);
    assert_eq!(info.channels, 2);
    
    // Lire quelques samples
    let mut buffer = vec![0u8; 4096];
    let mut reader = stream;
    let n = reader.read(&mut buffer).await.unwrap();
    assert!(n > 0);
}
```

**Tests intégration** :
```rust
#[tokio::test]
#[ignore = "Integration test - network required"]
async fn test_transcode_radiofrance_to_flac() {
    let stream_url = "https://icecast.radiofrance.fr/fip-hifi.aac";
    let response = reqwest::get(stream_url).await.unwrap();
    let reader = response.bytes_stream();
    
    let transcoded = transcode_to_flac_stream(
        reader,
        TranscodeOptions::default()
    ).await.unwrap();
    
    assert_eq!(transcoded.input_codec(), AudioCodec::Aac);
    assert_eq!(transcoded.input_stream_info().sample_rate, 48000);
    
    // Lire 5 secondes de FLAC
    let mut output = Vec::new();
    let mut stream = transcoded.into_stream();
    
    for _ in 0..50 {
        let mut chunk = vec![0u8; 8192];
        stream.read(&mut chunk).await.unwrap();
        output.extend_from_slice(&chunk);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    assert!(output.len() > 100_000); // Au moins 100 KB de FLAC
}
```

#### Critères de succès Round 2

- ✅ `decode_aac_stream()` suit le pattern existant (MP3, Ogg, etc.)
- ✅ Auto-détection AAC/ADTS fonctionne
- ✅ Transcodage AAC → FLAC streaming opérationnel
- ✅ Tests unitaires et intégration passent
- ✅ Documentation complète (doctests, exemples)
- ✅ Feature flag `aac` pour compilation optionnelle

---

### Round 3 : Intégration dans pmoradiofrance (si Round 2 réussit)

**Objectif** : Remplacer le proxy AAC passthrough par un transcodage FLAC.

#### Modifications

**1. `pmoradiofrance/src/server_ext.rs`**

Remplacer le proxy passthrough par un transcodage :

```rust
async fn proxy_stream(
    Path(slug): Path<String>,
    State(state): State<Arc<RadioFranceServerState>>
) -> Result<Response, (StatusCode, String)> {
    let stream_url = state.client.get_stream_url(&slug).await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    
    let response = reqwest::get(&stream_url).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    
    // Transcoder AAC → FLAC avec pmoflac
    let transcoded = pmoflac::transcode_to_flac_stream(
        response.bytes_stream(),
        pmoflac::TranscodeOptions::default()
    ).await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    // Enregistrer connexion active et démarrer metadata refresh
    // ...
    
    // Stream FLAC au lieu d'AAC
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "audio/flac".parse().unwrap());
    headers.insert("Cache-Control", "no-cache".parse().unwrap());
    
    Ok((headers, Body::from_stream(transcoded.into_stream())).into_response())
}
```

**2. `pmoradiofrance/src/playlist.rs`**

Changer le protocol_info pour FLAC :

```rust
// Avant (AAC)
protocol_info: "http-get:*:audio/aac:*"

// Après (FLAC)
protocol_info: "http-get:*:audio/flac:*"
sample_frequency: Some(info.sample_rate.to_string())
bits_per_sample: Some("16".to_string())
```

#### Critères de succès Round 3

- ✅ Radio France sert du FLAC au lieu d'AAC
- ✅ Uniformité : toutes les sources PMOMusic servent du FLAC
- ✅ Latence acceptable (<2s) pour le streaming live
- ✅ CPU raisonnable pour 2-3 streams simultanés sur LAN
- ✅ Métadonnées volatiles toujours mises à jour

---

## Alternative : fdk-aac (si Symphonia échoue)

Si Symphonia ne fonctionne pas en streaming infini, explorer `fdk-aac` :

### Avantages
- ✅ Décodeur de référence (meilleure qualité)
- ✅ Conçu pour le streaming
- ✅ Utilisé en production (Android, etc.)

### Inconvénients
- ❌ Dépendance C (compilation complexe)
- ❌ Licence restrictive (vérifier compatibilité projet)

### Prototype minimal

```rust
use fdk_aac::dec::{Decoder, DecoderParams};

pub async fn decode_aac_with_fdk<R>(reader: R) -> Result<AacDecodedStream>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    // Similar pattern to pmoflac MP3 decoder
    // spawn_blocking pour le décodeur C
    // ...
}
```

---

## Résultats attendus

### Minimum viable (Round 1)

- ✅ Rapport technique sur la faisabilité du streaming AAC avec Symphonia
- ✅ Prototype fonctionnel (même basique)
- ✅ Identification des limitations et solutions de contournement

### Objectif complet (Round 1-3)

- ✅ Support AAC/ADTS dans `pmoflac` (feature flag optionnelle)
- ✅ Transcodage AAC → FLAC streaming opérationnel
- ✅ Radio France servant du FLAC uniforme
- ✅ Documentation et tests complets

### En cas d'échec

- ✅ Rapport détaillé des blocages techniques
- ✅ Recommandations alternatives (fdk-aac, attendre évolution Symphonia, etc.)
- ✅ Garder le proxy AAC passthrough actuel

---

## Références

### Documentation
- [Symphonia Getting Started](https://github.com/pdeljanov/Symphonia/blob/master/GETTING_STARTED.md)
- [AdtsReader API](https://docs.rs/symphonia-codec-aac/latest/symphonia_codec_aac/struct.AdtsReader.html)
- [ADTS Format Specification](https://wiki.multimedia.cx/index.php/ADTS)
- [fdk-aac Rust Bindings](https://github.com/haileys/fdk-aac-rs)

### Issues et discussions
- [Symphonia ReadOnlySource Issue #580](https://github.com/RustAudio/rodio/issues/580)
- [Symphonia MediaSource Trait](https://docs.rs/symphonia-core/latest/symphonia_core/io/index.html)

### Contexte PMOMusic
- [Task Radio France](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/ToDiscuss/Construire_pmoradiofrance.md)
- [Architecture pmoflac](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoflac/src/lib.rs)
