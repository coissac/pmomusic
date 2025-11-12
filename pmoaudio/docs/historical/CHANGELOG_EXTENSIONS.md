# Changelog - Extensions Multiroom et Volume

## Version 0.2.0 - Extensions Multiroom

### Nouvelles fonctionnalités

#### 1. Contrôle de volume dynamique
- **VolumeNode** : node de contrôle de volume software thread-safe
- **HardwareVolumeNode** : variant pour contrôle matériel (prévu)
- **VolumeHandle** : handle pour contrôler le volume depuis un autre contexte
- **Système master/slave** : synchronisation automatique du volume entre branches

#### 2. Nouveaux types de sinks
- **DiskSink** : écriture sur disque (WAV, FLAC, PCM)
  - Dérivation automatique du nom de fichier depuis la source
  - Application automatique du gain avant écriture
- **ChromecastSink** : diffusion vers Chromecast (mock)
- **MpdSink** : streaming vers MPD (mock)

#### 3. Système d'événements
- **EventPublisher/EventReceiver** : système d'abonnement générique type-safe
- **VolumeChangeEvent** : notification de changement de volume
- **SourceNameUpdateEvent** : mise à jour du nom de source
- **AudioDataEvent** : transport de données audio via événements

#### 4. Extensions AudioChunk
- Nouveau champ `gain: f32` pour contrôle de volume lazy
- `with_gain()` : constructeur avec gain
- `apply_gain()` : application du gain sur les samples
- `with_modified_gain()` : modification du gain sans copie

### Modules ajoutés
```
src/
├── events.rs                         [NOUVEAU]
└── nodes/
    ├── volume_node.rs                [NOUVEAU]
    ├── disk_sink.rs                  [NOUVEAU]
    ├── chromecast_sink.rs            [NOUVEAU]
    └── mpd_sink.rs                   [NOUVEAU]

examples/
├── volume_control_demo.rs            [NOUVEAU]
└── multiroom_volume_demo.rs          [NOUVEAU]
```

### API publique

#### Exports ajoutés dans lib.rs
```rust
// Events
pub use events::{
    AudioDataEvent,
    EventPublisher,
    EventReceiver,
    NodeEvent,
    NodeListener,
    SourceNameUpdateEvent,
    VolumeChangeEvent,
};

// Volume nodes
pub use nodes::volume_node::{
    HardwareVolumeNode,
    VolumeHandle,
    VolumeNode,
};

// Sinks
pub use nodes::disk_sink::{
    AudioFileFormat,
    DiskSink,
    DiskSinkConfig,
    DiskSinkStats,
};

pub use nodes::chromecast_sink::{
    ChromecastConfig,
    ChromecastSink,
    ChromecastStats,
    StreamEncoding,
};

pub use nodes::mpd_sink::{
    MpdAudioFormat,
    MpdConfig,
    MpdHandle,
    MpdSink,
    MpdStats,
};
```

### Modifications de types existants

#### AudioChunk
```rust
pub struct AudioChunk {
    pub order: u64,
    pub left: Arc<Vec<f32>>,
    pub right: Arc<Vec<f32>>,
    pub sample_rate: u32,
    pub gain: f32,              // [NOUVEAU]
}

impl AudioChunk {
    // Méthodes existantes (inchangées)
    pub fn new(...) -> Self;
    pub fn from_arc(...) -> Self;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn clone_data(&self) -> (Vec<f32>, Vec<f32>);

    // Nouvelles méthodes
    pub fn with_gain(..., gain: f32) -> Self;         // [NOUVEAU]
    pub fn from_arc_with_gain(..., gain: f32) -> Self;// [NOUVEAU]
    pub fn apply_gain(&self) -> Self;                 // [NOUVEAU]
    pub fn with_modified_gain(&self, new_gain: f32) -> Self; // [NOUVEAU]
}
```

### Tests
- 12 nouveaux tests unitaires
- Tous les tests existants continuent de passer
- **Total : 31 tests, 0 failures**

### Exemples
- `volume_control_demo` : contrôle de volume simple
- `multiroom_volume_demo` : pipeline multiroom complet

### Breaking changes
**Aucun** - Toutes les modifications sont additives.

### Performances
- **Zero-copy maintenu** : partage des `Arc<AudioChunk>` entre branches
- **Lazy evaluation** : gain non appliqué jusqu'au sink
- **Thread-safe** : `RwLock` pour le volume, channels Tokio

### Documentation
- `FEATURES_EXTENDED.md` : documentation complète des fonctionnalités
- `IMPLEMENTATION_SUMMARY.md` : résumé technique de l'implémentation
- Commentaires inline dans le code

---

## Migration depuis 0.1.0

Aucune migration nécessaire. Le code existant fonctionne sans modification.

### Pour utiliser les nouvelles fonctionnalités

#### Ajouter un contrôle de volume
```rust
// Avant
source.add_subscriber(sink_tx);

// Après
let (mut volume, volume_tx) = VolumeNode::new("main", 1.0, 10);
let handle = volume.get_handle();
volume.add_subscriber(sink_tx);
source.add_subscriber(volume_tx);

tokio::spawn(async move { volume.run().await });

// Modifier le volume dynamiquement
handle.set_volume(0.5).await;
```

#### Écrire sur disque
```rust
let config = DiskSinkConfig {
    output_dir: PathBuf::from("/tmp/audio"),
    filename: Some("output.wav".to_string()),
    ..Default::default()
};

let (disk_sink, disk_tx) = DiskSink::new("disk1".to_string(), config, 10);

// Connecter au pipeline
volume.add_subscriber(disk_tx);

// Lancer
tokio::spawn(async move {
    let stats = disk_sink.run().await.unwrap();
    stats.display();
});
```

#### Configuration multiroom
```rust
// Volume master
let (mut master, master_tx) = VolumeNode::new("master", 1.0, 50);
let (event_tx, event_rx1) = mpsc::channel(10);
let (_, event_rx2) = mpsc::channel(10);
master.subscribe_volume_events(event_tx);
source.add_subscriber(master_tx);

// Branche 1
let (mut vol1, vol1_tx) = VolumeNode::new("room1", 0.8, 50);
vol1.set_master_volume_source(event_rx1);
vol1.add_subscriber(sink1_tx);
master.add_subscriber(vol1_tx);

// Branche 2
let (mut vol2, vol2_tx) = VolumeNode::new("room2", 0.9, 50);
vol2.set_master_volume_source(event_rx2);
vol2.add_subscriber(sink2_tx);
master.add_subscriber(vol2_tx);

// Contrôle master
let master_handle = master.get_handle();
master_handle.set_volume(0.7).await; // Affecte toutes les branches
```

---

## Roadmap

### v0.3.0 (prévu)
- [ ] Implémentation réelle ChromecastSink avec `rust-cast`
- [ ] Implémentation réelle MpdSink avec protocole MPD
- [ ] Support FLAC dans DiskSink avec `claxon`
- [ ] AirPlaySink (diffusion AirPlay/AirPlay 2)
- [ ] EqualizerNode (égaliseur paramétrique)

### v0.4.0 (prévu)
- [ ] PulseAudioSink / AlsaSink / CoreAudioSink
- [ ] CompressorNode / LimiterNode (dynamiques)
- [ ] ReverbNode (réverbération)
- [ ] CrossfadeNode (transition entre sources)
- [ ] HttpStreamSink (serveur Icecast/Shoutcast)

### v1.0.0 (futur)
- [ ] Synchronisation NTP/PTP pour multi-device
- [ ] Room correction avec FIR filters
- [ ] API REST pour contrôle
- [ ] Dashboard web
- [ ] Documentation complète utilisateur

---

## Contributeurs
- Implémentation initiale : Assistant Claude
- Architecture PMOAudio : Projet PMOMusic

## Licence
Partie du projet PMOMusic
