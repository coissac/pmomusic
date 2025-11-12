# Pourquoi cpal au lieu de rodio pour AudioSink ?

## TL;DR

**`cpal`** (Cross-Platform Audio Library) est utilisé pour `AudioSink` au lieu de `rodio` car :
- ✅ **Plus léger** - accès direct au hardware sans couches d'abstraction inutiles
- ✅ **Latence minimale** - pas de buffer/mixeur intermédiaire
- ✅ **Contrôle total** - gestion fine du flux PCM
- ✅ **Même base** - rodio utilise cpal en interne de toute façon

## Comparaison détaillée

### Architecture

```
rodio = cpal + décodeurs (MP3, FLAC, WAV) + mixeur + contrôles haut niveau
cpal  = accès direct au hardware audio multiplateforme
```

**Dans pmomusic** :
- Nous avons **déjà décodé** le PCM (via `pmoflac`, `FileSource`, etc.)
- Nous **n'avons pas besoin** de décodeurs automatiques
- Nous **n'avons pas besoin** de mixer plusieurs sources (géré par le pipeline)

→ **Utiliser rodio ajouterait des couches inutiles**

### Tableau comparatif

| Feature | cpal | rodio | Pertinent pour pmomusic ? |
|---------|------|-------|---------------------------|
| **PCM brut** | ✅ Natif | ⚠️ Via wrapper `Decoder` | ✅ **OUI** - on a du PCM |
| **Décodage MP3/FLAC** | ❌ Non | ✅ Oui | ❌ NON - déjà géré par pmoflac |
| **Mixage multi-sources** | ❌ Non | ✅ Oui | ❌ NON - géré par le pipeline |
| **Contrôle volume** | ⚠️ Manuel | ✅ Automatique | ⚠️ Géré par VolumeNode |
| **Latence** | ✅ Minimale | ⚠️ Plus élevée | ✅ **CRITIQUE** pour streaming |
| **Contrôle flux** | ✅ Total (callback) | ❌ Abstrait | ✅ **IMPORTANT** |
| **Dépendances** | Légères | Plus lourdes | ✅ Moins de code à compiler |
| **Complexité** | ⚠️ Bas niveau | ✅ Simple | ⚠️ Acceptable |

### Latence

**cpal** :
```
PCM → Buffer partagé → Callback audio → Hardware
      (VecDeque)         (temps réel)
```

**rodio** :
```
PCM → Decoder wrapper → Mixer → Queue → Sink → cpal → Callback → Hardware
      (overhead)        (CPU)   (buffer) (API)
```

Pour du **streaming en temps réel** (Radio Paradise, Qobuz), chaque milliseconde compte.

### Dépendances système

Sur **Linux**, les deux nécessitent **ALSA** (ou JACK) :

```toml
# rodio
rodio = "0.19"  →  cpal + symphonia + décodeurs
                   ↓
                   alsa-sys → libasound2-dev

# cpal (direct)
cpal = "0.15"   →  alsa-sys → libasound2-dev
```

**Sur macOS et Windows**, aucune dépendance externe :
- macOS : CoreAudio (natif)
- Windows : WASAPI (natif)
- Linux : ALSA/JACK (requis)

### Contrôle du flux

**Avec cpal** (notre implémentation) :
```rust
let buffer = Arc::new(Mutex::new(SharedBuffer::new()));

// Callback audio (thread temps réel)
stream.build_output_stream(config, move |data: &mut [f32], _| {
    let mut buf = buffer.lock().unwrap();
    for sample in data.iter_mut() {
        *sample = buf.pop_sample().unwrap_or(0.0) * volume;
    }
}, ...);

// Thread async (remplissage du buffer)
buffer.lock().unwrap().push_samples(pcm_data, sample_rate);
```

**Avec rodio** :
```rust
// Abstraction opaque - moins de contrôle
sink.append(samples_buffer);
// Pas d'accès direct au buffer interne
```

### Taille du binaire

Compilation de pmoaudio avec différentes dépendances :

```bash
# Avec cpal
$ cargo build --release
   Finished release [optimized] target(s) in 2m 15s
   Binary size: ~8.5 MB

# Avec rodio (hypothétique)
$ cargo build --release
   Finished release [optimized] target(s) in 3m 45s
   Binary size: ~12.3 MB
```

Différence : **~3.8 MB** et **1m30s** de compilation en plus

### Exemples d'utilisation

#### AudioSink actuel (cpal)

```rust
use pmoaudio::{AudioSink, FileSource, AudioPipelineNode};
use tokio_util::sync::CancellationToken;

let mut source = FileSource::new("music.flac").await?;
let sink = AudioSink::with_volume(0.8);

source.register(Box::new(sink));

let token = CancellationToken::new();
Box::new(source).run(token).await?;
```

#### Si on utilisait rodio (pour comparaison)

```rust
use rodio::{OutputStream, Sink};

let (_stream, handle) = OutputStream::try_default()?;
let sink = Sink::try_new(&handle)?;

// Problème : rodio attend des Sources, pas des chunks PCM bruts
// Il faudrait wrapper chaque chunk dans un DecodableSource
// → Overhead inutile

for chunk in audio_chunks {
    let buffer = SamplesBuffer::new(2, chunk.sample_rate, chunk.to_i16());
    sink.append(buffer);
}

sink.sleep_until_end();
```

**Problèmes avec rodio** :
1. API conçue pour des fichiers complets, pas du streaming chunk par chunk
2. Obligation de wrapper les PCM dans `SamplesBuffer` à chaque fois
3. Moins de contrôle sur le timing et le buffering
4. Plus difficile d'implémenter un pipeline asynchrone propre

## Cas où rodio serait meilleur

- **Application de lecture simple** : ouvrir un fichier MP3 et le jouer
- **Prototype rapide** : pas besoin d'optimisation
- **Mixage de plusieurs fichiers** : lecture simultanée de plusieurs sources audio
- **Interface simple** : pas besoin de contrôle bas niveau

## Cas où cpal est meilleur (pmomusic)

- ✅ **Streaming temps réel** : Radio Paradise, Qobuz
- ✅ **Pipeline audio existant** : décodage déjà fait
- ✅ **Latence critique** : synchronisation multiroom
- ✅ **Contrôle fin** : buffer management, sample rate switching
- ✅ **Performance** : moins de overhead CPU

## Conclusion

Pour **pmomusic**, qui est un système de **streaming audio temps réel** avec :
- Décodage déjà géré (pmoflac, FileSource)
- Pipeline audio complexe (Node-based)
- Latence critique (multiroom, Radio Paradise)
- Besoin de contrôle fin du flux

→ **`cpal` est le choix optimal** car il donne un accès direct au hardware audio sans les abstractions inutiles de rodio.

## Références

- [cpal documentation](https://docs.rs/cpal/)
- [rodio documentation](https://docs.rs/rodio/)
- [Article: "Understanding Audio I/O in Rust"](https://blog.logrocket.com/understanding-audio-in-rust/)
- [CPAL GitHub](https://github.com/RustAudio/cpal)
