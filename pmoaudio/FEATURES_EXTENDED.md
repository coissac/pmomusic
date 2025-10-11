# PMOAudio - Extensions Multiroom et Contrôle de Volume

## Vue d'ensemble

Ce document décrit les extensions apportées au système PMOAudio pour supporter :
- **Contrôle de volume** dynamique avec synchronisation master/secondaire
- **Nouveaux types de sinks** : DiskSink, ChromecastSink, MpdSink
- **Système d'événements générique** pour la communication inter-nodes
- **Champ gain** dans AudioChunk pour le contrôle du volume en pipeline

---

## 1. AudioChunk avec gain

Le type `AudioChunk` a été étendu avec un champ `gain: f32` qui permet de contrôler le volume de manière lazy (le gain est appliqué au moment voulu, pas immédiatement).

### Nouvelles méthodes

```rust
// Créer un chunk avec gain spécifique
let chunk = AudioChunk::with_gain(0, left, right, 48000, 0.5);

// Modifier le gain d'un chunk existant (cheap, pas de copie)
let modified = chunk.with_modified_gain(0.8);

// Appliquer le gain et matérialiser les données modifiées
let applied = chunk.apply_gain();
```

### Comportement

- Le gain par défaut est `1.0` (aucun changement)
- Les gains se multiplient en cascade (utile pour chaîner plusieurs VolumeNode)
- `apply_gain()` crée un nouveau chunk avec les samples multipliés par le gain

---

## 2. Système d'événements générique

Un système d'abonnement type-safe permet aux nodes d'émettre et de recevoir différents types d'événements.

### Types d'événements disponibles

```rust
// Événement de changement de volume
VolumeChangeEvent {
    volume: f32,
    source_node_id: String,
}

// Événement de mise à jour du nom de source
SourceNameUpdateEvent {
    source_name: String,
    device_name: Option<String>,
}

// Événement de données audio (pour référence)
AudioDataEvent {
    chunk: Arc<AudioChunk>,
}
```

### Utilisation

```rust
// Créer un publisher
let mut volume_publisher = EventPublisher::<VolumeChangeEvent>::new();

// S'abonner
let (tx, mut rx) = mpsc::channel(10);
volume_publisher.subscribe(tx);

// Publier un événement
let event = VolumeChangeEvent {
    volume: 0.7,
    source_node_id: "master".to_string(),
};
volume_publisher.publish(event).await;

// Recevoir
let received = rx.recv().await;
```

---

## 3. VolumeNode - Contrôle de volume software

Le `VolumeNode` permet d'ajuster dynamiquement le volume du flux audio.

### Caractéristiques

- **Thread-safe** : le volume peut être modifié pendant l'exécution
- **Notification** : émet des événements lors des changements
- **Master/Slave** : peut s'abonner à un volume master
- **Lazy application** : modifie le champ `gain` du chunk, pas les données

### Exemple de base

```rust
// Créer un VolumeNode avec volume initial 0.8
let (mut volume_node, volume_tx) = VolumeNode::new(
    "room1".to_string(),
    0.8,  // volume initial
    10    // taille du channel
);

// Obtenir un handle pour contrôler le volume
let handle = volume_node.get_handle();

// Modifier le volume depuis un autre contexte
tokio::spawn(async move {
    handle.set_volume(0.5).await;
});

// Lancer le node
tokio::spawn(async move {
    volume_node.run().await.unwrap()
});
```

### Configuration Master/Slave

```rust
// Créer le master
let (mut master, master_tx) = VolumeNode::new("master".to_string(), 1.0, 10);
let (master_event_tx, master_event_rx) = mpsc::channel(10);
master.subscribe_volume_events(master_event_tx);
let master_handle = master.get_handle();

// Créer le slave
let (mut slave, slave_tx) = VolumeNode::new("slave".to_string(), 0.8, 10);
slave.set_master_volume_source(master_event_rx);

// Le slave appliquera maintenant: local_volume * master_volume
// Ex: si master=0.5 et local=0.8, le gain final sera 0.4
```

---

## 4. HardwareVolumeNode

Version spécialisée pour contrôle hardware du volume (via driver audio).

**Note** : L'implémentation actuelle est identique à `VolumeNode`. Dans une vraie implémentation, elle communiquerait avec le driver système (ALSA, CoreAudio, WASAPI, etc.).

```rust
let (hw_volume, hw_tx) = HardwareVolumeNode::new(
    "hardware".to_string(),
    0.8,
    10
);

let handle = hw_volume.get_handle();
handle.set_volume(0.9).await;  // Ajusterait le volume matériel
```

---

## 5. DiskSink - Écriture sur disque

Le `DiskSink` écrit le flux audio dans un fichier sur disque avec support de plusieurs formats.

### Caractéristiques

- **Dérivation automatique du nom** : peut utiliser le nom de la source
- **Formats supportés** : WAV, FLAC (mock), PCM brut
- **Application du gain** : applique automatiquement le gain avant l'écriture
- **Écriture asynchrone** avec buffer

### Configuration

```rust
let config = DiskSinkConfig {
    output_dir: PathBuf::from("/tmp/audio"),
    filename: Some("output.wav".to_string()),  // ou None pour dérivation auto
    format: AudioFileFormat::Wav,
    buffer_size: 100,
};

let (disk_sink, disk_tx) = DiskSink::new("disk1".to_string(), config, 10);
```

### Dérivation du nom de fichier

Si `filename` est `None`, le DiskSink peut écouter les événements `SourceNameUpdateEvent` pour dériver automatiquement le nom :

```rust
let (source_name_tx, source_name_rx) = mpsc::channel(10);
disk_sink.set_source_name_source(source_name_rx);

// Quand un événement est reçu
let event = SourceNameUpdateEvent {
    source_name: "My_Song.mp3".to_string(),
    device_name: None,
};
source_name_tx.send(event).await;

// Le fichier sera créé comme: /tmp/audio/My_Song_mp3.wav
```

### Formats supportés

```rust
// WAV (16-bit PCM stéréo)
AudioFileFormat::Wav

// FLAC (nécessite bibliothèque externe - actuellement utilise WAV)
AudioFileFormat::Flac

// PCM brut (pas d'en-tête)
AudioFileFormat::Raw
```

---

## 6. ChromecastSink - Diffusion Chromecast

Streame l'audio vers un périphérique Chromecast.

**Note** : Implémentation mock. Une vraie implémentation nécessiterait une bibliothèque comme `rust-cast`.

### Configuration

```rust
let config = ChromecastConfig {
    device_address: "192.168.1.100".to_string(),
    device_name: "Living Room".to_string(),
    port: 8009,
    buffer_size: 50,
    encoding: StreamEncoding::Mp3,
};

let (chromecast_sink, chromecast_tx) = ChromecastSink::new(
    "chromecast1".to_string(),
    config,
    10
);
```

### Encodages supportés

```rust
StreamEncoding::Mp3   // Compatible avec la plupart des Chromecasts
StreamEncoding::Aac   // Haute qualité
StreamEncoding::Opus  // Faible latence
StreamEncoding::Pcm   // Non compressé (haute bande passante)
```

---

## 7. MpdSink - Streaming vers MPD

Envoie le flux à un démon MPD (Music Player Daemon).

**Note** : Implémentation mock. Une vraie implémentation nécessiterait le protocole MPD complet.

### Configuration

```rust
let config = MpdConfig {
    host: "localhost".to_string(),
    port: 6600,
    password: Some("secret".to_string()),
    output_name: Some("ALSA".to_string()),
    buffer_size: 50,
    format: MpdAudioFormat::S16Le,
};

let (mpd_sink, mpd_tx) = MpdSink::new("mpd1".to_string(), config, 10);
```

### Contrôle MPD

Le MpdSink fournit un handle pour contrôler la lecture :

```rust
let handle = mpd_sink.get_handle();

handle.play().await;
handle.pause().await;
handle.set_volume(75).await;  // 0-100
handle.stop().await;
```

### Formats audio MPD

```rust
MpdAudioFormat::S16Le  // 16-bit signed
MpdAudioFormat::S24Le  // 24-bit signed
MpdAudioFormat::S32Le  // 32-bit signed
MpdAudioFormat::F32    // Float 32-bit
```

---

## 8. Pipeline Multiroom Complet

Voici un exemple complet d'utilisation de toutes les fonctionnalités :

```rust
use pmoaudio::{
    SourceNode, VolumeNode, ChromecastSink, DiskSink,
    ChromecastConfig, DiskSinkConfig,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Source audio
    let mut source = SourceNode::new();

    // 2. Volume master
    let (mut master_volume, master_tx) = VolumeNode::new("master".to_string(), 1.0, 50);
    let master_handle = master_volume.get_handle();
    let (master_event_tx, master_event_rx_chromecast) = mpsc::channel(10);
    let (_, master_event_rx_disk) = mpsc::channel(10);
    master_volume.subscribe_volume_events(master_event_tx);
    source.add_subscriber(master_tx);

    // 3. Branche Chromecast avec volume secondaire
    let (mut chromecast_volume, chromecast_volume_tx) =
        VolumeNode::new("chromecast_volume".to_string(), 0.8, 50);
    chromecast_volume.set_master_volume_source(master_event_rx_chromecast);

    let chromecast_config = ChromecastConfig {
        device_address: "192.168.1.100".to_string(),
        device_name: "Living Room".to_string(),
        ..Default::default()
    };
    let (chromecast_sink, chromecast_sink_tx) =
        ChromecastSink::new("chromecast1".to_string(), chromecast_config, 50);

    chromecast_volume.add_subscriber(chromecast_sink_tx);
    master_volume.add_subscriber(chromecast_volume_tx);

    // 4. Branche DiskSink avec volume secondaire
    let (mut disk_volume, disk_volume_tx) =
        VolumeNode::new("disk_volume".to_string(), 0.9, 50);
    disk_volume.set_master_volume_source(master_event_rx_disk);

    let disk_config = DiskSinkConfig {
        output_dir: std::env::temp_dir().join("audio"),
        filename: Some("output.wav".to_string()),
        ..Default::default()
    };
    let (disk_sink, disk_sink_tx) =
        DiskSink::new("disk1".to_string(), disk_config, 50);

    disk_volume.add_subscriber(disk_sink_tx);
    master_volume.add_subscriber(disk_volume_tx);

    // 5. Lancer tous les nodes
    tokio::spawn(async move { master_volume.run().await.unwrap() });
    tokio::spawn(async move { chromecast_volume.run().await.unwrap() });
    tokio::spawn(async move { disk_volume.run().await.unwrap() });

    let chromecast_handle = tokio::spawn(async move {
        chromecast_sink.run().await.unwrap()
    });
    let disk_handle = tokio::spawn(async move {
        disk_sink.run().await.unwrap()
    });

    // 6. Contrôler le volume dynamiquement
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        master_handle.set_volume(0.7).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        master_handle.set_volume(0.4).await;
    });

    // 7. Générer et streamer l'audio
    tokio::spawn(async move {
        source.generate_chunks(50, 4800, 48000, 440.0).await.unwrap();
    });

    // 8. Attendre la fin
    chromecast_handle.await?;
    disk_handle.await?;

    Ok(())
}
```

---

## Architecture du pipeline multiroom

```text
┌──────────────┐
│  SourceNode  │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ MasterVolume │ ───────► VolumeChangeEvent
└──────┬───────┘              │
       │                      │
       ├──────────────────────┼────────────┐
       ▼                      ▼            ▼
┌─────────────────┐    ┌──────────────┐   │
│ChromecastVolume │    │  DiskVolume  │   │
│   (0.8 local)   │    │  (0.9 local) │   │
└────────┬────────┘    └──────┬───────┘   │
         │                    │            │
         │ gain=master×local  │            │
         ▼                    ▼            ▼
┌─────────────────┐    ┌──────────────┐  ...
│ ChromecastSink  │    │   DiskSink   │
│  Living Room    │    │  output.wav  │
└─────────────────┘    └──────────────┘
```

### Flux des données

1. **SourceNode** génère des chunks audio avec `gain = 1.0`
2. **MasterVolume** modifie le gain : `chunk.gain *= master_volume`
3. Chaque **branche secondaire** :
   - Reçoit le chunk du master
   - Applique son volume local : `chunk.gain *= local_volume`
   - Envoie au sink
4. Les **sinks** appliquent le gain final avant l'output

---

## Optimisations

### Zero-copy jusqu'au bout

- Les chunks audio (`Arc<AudioChunk>`) sont partagés entre branches
- Seule la structure est clonée (cheap), pas les données audio
- Le gain est stocké dans le chunk, pas appliqué immédiatement

### Application lazy du gain

```rust
// Modification du gain : O(1), pas de copie
let modified = chunk.with_modified_gain(0.5);

// Application : O(n), copie et multiplie les samples
let applied = chunk.apply_gain();
```

### Thread-safety

- `VolumeHandle` utilise `Arc<RwLock<f32>>` pour partager le volume
- Changements de volume thread-safe et non-bloquants
- `EventPublisher` utilise `try_send` pour éviter les blocages

---

## Tests

Tous les composants incluent des tests unitaires :

```bash
cargo test --lib
```

### Tests disponibles

- `test_volume_node_basic` : test de base du VolumeNode
- `test_volume_handle` : modification du volume via handle
- `test_volume_events` : publication d'événements
- `test_master_slave_volume` : synchronisation master/slave
- `test_disk_sink_basic` : écriture sur disque
- `test_chromecast_sink_basic` : simulation Chromecast
- `test_mpd_sink_basic` : simulation MPD

---

## Exemples

Deux exemples complets sont fournis :

### 1. Volume Control Demo

Démontre le contrôle dynamique du volume :

```bash
cargo run --example volume_control_demo
```

### 2. Multiroom Volume Demo

Démontre un pipeline complet avec deux branches et synchronisation master/slave :

```bash
cargo run --example multiroom_volume_demo
```

---

## Évolutions futures

### Implémentations réelles des sinks

1. **ChromecastSink** : intégrer `rust-cast` ou équivalent
2. **MpdSink** : implémenter le protocole MPD complet
3. **DiskSink FLAC** : intégrer `flac` ou `symphonia`

### Nouveaux sinks possibles

- `AirPlaySink` : diffusion vers AirPlay/AirPlay 2
- `PulseAudioSink` : sortie vers PulseAudio
- `AlsaSink` : sortie directe ALSA (Linux)
- `CoreAudioSink` : sortie CoreAudio (macOS)
- `WasapiSink` : sortie WASAPI (Windows)
- `HttpStreamSink` : serveur HTTP pour streaming
- `RtpSink` : streaming RTP/UDP

### Fonctionnalités avancées

- **Égaliseur** : `EqualizerNode` avec bandes paramétriques
- **Compresseur/Limiteur** : `DynamicsNode`
- **Crossfade** : transition entre sources
- **Room correction** : correction acoustique par pièce
- **Synchronisation multi-device** : timing précis avec NTP/PTP

---

## Licence

Ce code fait partie du projet PMOMusic.
