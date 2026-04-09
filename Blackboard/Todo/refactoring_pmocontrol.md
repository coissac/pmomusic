# Refactoring Plan - Crate `pmocontrol`

## Contexte

La crate `pmocontrol` implémente un control point UPnP multiprotocole pour contrôler des renderers audio (UPnP/DLNA, OpenHome, LinkPlay, Arylic TCP, Chromecast). Une refactorisation récente avait pour objectif de monter la logique vers les couches abstraites, mais des duplications et des problèmes de conception subsistent.

---

## Structure analysée

- `music_renderer/` : Implémentations concrètes + façade `MusicRenderer`
- `queue/` : Gestion abstraite et concrète des files de lecture
- `discovery/` : Découverte SSDP et gestion des appareils
- `upnp_clients/` : Clients SOAP pour services UPnP
- `control_point.rs` : Point de contrôle principal

Backends : `UpnpRenderer`, `OpenHomeRenderer`, `LinkPlayRenderer`, `ArylicTcpRenderer`, `ChromecastRenderer`, `HybridUpnpArylicRenderer`

---

## CATÉGORIE P0 : BUGS LOGIQUES (à corriger immédiatement)

### BUG-1 : `sync_queue` dans UpnpRenderer ignore le cancel_token

**Fichier :** `src/music_renderer/upnp_renderer.rs` (lignes ~354-364)

**Description :** Le paramètre `cancel_token` est reçu comme `_cancel_token` (ignoré) et remplacé par un `Arc::new(AtomicBool::new(false))` fraîchement créé. Les demandes d'annulation de synchronisation de queue sont silencieusement ignorées pour le backend UPnP.

**Correction :**
```rust
fn sync_queue(
    &mut self,
    items: Vec<PlaybackItem>,
    cancel_token: &Arc<AtomicBool>,  // utiliser le param, pas _cancel_token
    on_ready: Option<Box<dyn FnOnce() + Send>>,
) -> Result<(), ControlPointError> {
    self.queue
        .lock()
        .unwrap()
        .sync_queue(items, cancel_token, on_ready)  // passer le vrai token
}
```

**Tâche :** Vérifier également les autres backends (OpenHome, LinkPlay, Arylic, Chromecast) s'ils propagent correctement le cancel_token.

---

## CATÉGORIE P1 : DUPLICATIONS MAJEURES (à traiter en priorité)

### DUP-1 : Implémentation de `QueueBackend` répétée dans les 5+ renderers

**Fichiers :**
- `src/music_renderer/upnp_renderer.rs` (~310-381)
- `src/music_renderer/arylic_tcp.rs` (~368-450)
- `src/music_renderer/linkplay_renderer.rs` (~246-330)
- `src/music_renderer/chromecast_renderer.rs` (~866+)
- `src/music_renderer/openhome_renderer.rs` (~629+)

**Description :** Chaque renderer implémente `QueueBackend` de manière identique : chaque méthode verrouille `self.queue` et délègue à la file sous-jacente. ~150+ lignes de boilerplate.

**Approche recommandée — Trait délégateur :**
```rust
// Dans queue/mod.rs ou music_renderer/mod.rs
pub trait HasQueue {
    fn queue(&self) -> &Arc<Mutex<MusicQueue>>;
}

// Impl automatique pour QueueBackend si le type implémente HasQueue
impl<T: HasQueue> QueueBackend for T {
    fn len(&self) -> Result<usize, ControlPointError> {
        self.queue().lock().unwrap().len()
    }
    fn track_ids(&self) -> Result<Vec<u32>, ControlPointError> {
        self.queue().lock().unwrap().track_ids()
    }
    // ... toutes les méthodes déléguantes
}

// Dans chaque renderer : une seule ligne
impl HasQueue for UpnpRenderer {
    fn queue(&self) -> &Arc<Mutex<MusicQueue>> { &self.queue }
}
```

**Tâche :** Définir le trait `HasQueue`, implémenter `QueueBackend for T where T: HasQueue`, supprimer les implémentations manuelles dans chaque renderer.

---

### DUP-2 : Logique commune de `play_from_queue` dupliquée dans 4+ renderers

**Fichiers :**
- `src/music_renderer/upnp_renderer.rs` (~184-256)
- `src/music_renderer/linkplay_renderer.rs` (~189-212)
- `src/music_renderer/arylic_tcp.rs` (~311-334)
- `src/music_renderer/openhome_renderer.rs` (~partie similaire)

**Description :** Les 10-12 premières lignes de `play_from_queue` sont identiques dans tous les renderers : verrouillage de queue, gestion de l'index courant, fallback sur index 0 si non défini, récupération de l'item. Seule la partie terminale (play effectif sur le backend) diffère.

**Approche recommandée — Méthode par défaut dans un trait :**
```rust
pub trait QueueTransportControl: HasQueue + HasContinuousStream {
    // Primitive spécifique au backend
    fn play_item(&self, item: &PlaybackItem) -> Result<(), ControlPointError>;

    // Implémentation commune par défaut
    fn play_from_queue(&self) -> Result<(), ControlPointError> {
        let mut queue = self.queue().lock().unwrap();
        let current_index = match queue.current_index()? {
            Some(idx) => idx,
            None => {
                if queue.len()? > 0 {
                    queue.set_index(Some(0))?;
                    0
                } else {
                    return Err(ControlPointError::QueueError("Queue is empty".into()));
                }
            }
        };
        let item = queue.get_item(current_index)?
            .ok_or_else(|| ControlPointError::QueueError("Current item not found".into()))?;
        drop(queue);

        let is_stream = is_continuous_stream_url(&item.uri);
        *self.continuous_stream().lock().unwrap() = is_stream;
        self.play_item(&item)
    }
}
```

**Tâche :** Créer `QueueTransportControl` avec une méthode par défaut, implémenter `play_item` dans chaque renderer, supprimer la logique commune dupliquée.

---

### DUP-3 : Initialisation redondante des champs partagés dans tous les renderers

**Fichiers :** Constructeurs dans tous les fichiers renderer

**Description :** Chaque renderer répète la même construction :
```rust
let queue = Arc::new(Mutex::new(MusicQueue::from_renderer_info(info)?));
// ...
continuous_stream: Arc::new(Mutex::new(false)),
```

**Approche recommandée :**
```rust
pub struct SharedRendererState {
    pub queue: Arc<Mutex<MusicQueue>>,
    pub continuous_stream: Arc<Mutex<bool>>,
}

impl SharedRendererState {
    pub fn from_renderer_info(info: &RendererInfo) -> Result<Self, ControlPointError> {
        Ok(Self {
            queue: Arc::new(Mutex::new(MusicQueue::from_renderer_info(info)?)),
            continuous_stream: Arc::new(Mutex::new(false)),
        })
    }
}
```

**Tâche :** Créer `SharedRendererState`, l'utiliser dans tous les constructeurs de renderers.

---

### DUP-4 : `parse_didl_duration` implémentée deux fois différemment

**Fichiers :**
- `src/music_renderer/upnp_renderer.rs` (~383-420) : parsing manuel par string search (fragile)
- `src/music_renderer/musicrenderer.rs` (~2089-2117) : via parser DIDL-Lite structuré (robuste)

**Description :** Deux implémentations divergentes. L'une risque de mal parser du DIDL là où l'autre réussit.

**Tâche :** Conserver uniquement la version via `DIDLLite::parse`, l'exporter depuis `music_renderer/mod.rs`, supprimer la version par string search dans `upnp_renderer.rs`.

---

## CATÉGORIE P2 : ALGORITHMES COMPLEXES ET ABSTRACTIONS MAL PLACÉES

### ALGO-1 : `schedule_sync` dans `music_queue.rs` — logique intriquée

**Fichier :** `src/queue/music_queue.rs` (~97-200)

**Description :** La méthode crée un thread worker avec :
- Des `AtomicBool` pour synchronisation (sync_in_progress, sync_pending, sync_cancel_token)
- Une boucle infinie interne qui re-tente si un nouveau job arrive
- Un Guard RAII basé sur `Drop` pour le cleanup
- Des closures capturées mêlant synchronisation et logique métier

Difficile à tester, à observer de l'extérieur, pas de timeout.

**Tâche :**
1. Extraire la logique du worker dans une fonction `sync_worker_loop` avec signature claire
2. Documenter le protocole de synchronisation avec les AtomicBool
3. Ajouter une stratégie de timeout ou de sortie en cas de blocage

---

### ALGO-2 : Enum dispatch sprawl dans `MusicRendererBackend`

**Fichier :** `src/music_renderer/musicrenderer.rs` (~2134-2495)

**Description :** L'enum a 6 variantes. Chaque trait implémenté pour l'enum (`TransportControl`, `PlaybackStatus`, `PlaybackPosition`, `RendererBackend`, `QueueBackend`, etc.) contient un `match` sur les 6 variantes. Estimation : 200+ lignes de boilerplate purement mécanique. Ajouter une 7e variante requiert des mises à jour dans 25+ endroits.

**Approche recommandée — Macro de dispatch :**
```rust
macro_rules! dispatch {
    ($self:expr, $method:ident($($arg:expr),*)) => {
        match $self {
            MusicRendererBackend::Upnp(b) => b.$method($($arg),*),
            MusicRendererBackend::OpenHome(b) => b.$method($($arg),*),
            MusicRendererBackend::LinkPlay(b) => b.$method($($arg),*),
            MusicRendererBackend::ArylicTcp(b) => b.$method($($arg),*),
            MusicRendererBackend::Chromecast(b) => b.$method($($arg),*),
            MusicRendererBackend::HybridUpnpArylic { upnp, .. } => upnp.$method($($arg),*),
        }
    }
}

impl TransportControl for MusicRendererBackend {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<(), ControlPointError> {
        dispatch!(self, play_uri(uri, meta))
    }
    // ...
}
```

**Tâche :** Définir la macro `dispatch!`, remplacer les match statements redondants, valider les cas où HybridUpnpArylic a une logique spéciale.

---

### ALGO-3 : Logique de protection des durées de streams dupliquée dans 3 endroits

**Fichiers :**
- `src/queue/interne.rs` (~74-126) : `protect_stream_durations`
- `src/queue/openhome.rs` (~400+) : logique similaire pour playlists OpenHome
- `src/music_renderer/musicrenderer.rs` (~488-537) : dans `poll_and_emit_changes`

**Description :** La logique "refuser la diminution de durée pour un stream continu" est réimplémentée trois fois. Si la définition de "diminution acceptable" change, il faut modifier 3 fichiers.

**Tâche :** Créer `music_renderer/stream_utils.rs` (ou équivalent) avec une fonction `protect_stream_duration(old, new, is_stream) -> Option<String>` et l'utiliser dans les 3 endroits.

---

### ALGO-4 : Détection de flux continu fragmentée

**Fichiers :** `stream_detection.rs`, `musicrenderer.rs`, `queue/interne.rs`, `queue/openhome.rs`

**Description :** La détection "est-ce un stream continu?" passe par plusieurs chemins non unifiés :
1. `TrackMetadata::is_continuous_stream`
2. Appel `is_continuous_stream_url(uri)` (réseau)
3. Absence de durée dans les métadonnées

Un stream peut être marqué continu dans une couche mais pas l'autre.

**Tâche :** Créer une fonction canonique unique :
```rust
pub fn is_continuous_stream(metadata: Option<&TrackMetadata>, uri: &str) -> bool {
    metadata.map(|m| m.is_continuous_stream).unwrap_or(false)
    || is_continuous_stream_url(uri)
}
```
Faire passer tous les codepaths par cette fonction.

---

## CATÉGORIE P3 : BONNES PRATIQUES (amélioration continue)

### BP-1 : `.unwrap()` sur mutex locks (>50 occurrences)

**Problème :** Si un mutex est empoisonné (panique dans une autre tâche), `.unwrap()` propage la panique. Aucun code ne gère ce cas.

**Tâche :** Remplacer `.unwrap()` par `.expect("message contextuel")` à court terme. À long terme, envisager `parking_lot::Mutex` (pas de concept de poison).

---

### BP-2 : Absence de gestion d'erreur dans les threads watcher et sync

**Fichiers :** `musicrenderer.rs` (watcher_loop), `music_queue.rs` (schedule_sync)

**Tâche :** Ajouter `error!` logs dans les threads et décider explicitement de la politique de redémarrage (continuer vs arrêter).

---

### BP-3 : Champs `pub` au lieu de `pub(crate)` dans `PlaylistBinding`

**Fichier :** `src/music_renderer/musicrenderer.rs` (struct `PlaylistBinding`)

**Tâche :** Rendre les champs `pub` → `pub(crate)` ou privés avec accesseurs.

---

### BP-4 : Documentation manquante sur les contrats des traits

**Fichiers :** `src/music_renderer/capabilities.rs`, `src/queue/backend.rs`

**Tâche :** Ajouter des doc-comments sur les traits clés (`TransportControl`, `PlaybackStatus`, `QueueBackend`) décrivant les invariants, les pré/post-conditions, et le comportement attendu.

---

## PLAN D'EXÉCUTION

### Phase 1 — Bugs (immédiat)
- [ ] **BUG-1** : Corriger le cancel_token ignoré dans `upnp_renderer.rs::sync_queue`
- [ ] Vérifier les autres renderers pour le même bug

### Phase 2 — Éliminer les duplications majeures (1-2 semaines)
- [ ] **DUP-1** : Trait `HasQueue` + impl automatique de `QueueBackend`
- [ ] **DUP-4** : Unifier `parse_didl_duration` sur la version DIDL-Lite
- [ ] **DUP-3** : Créer `SharedRendererState` pour l'init commune
- [ ] **DUP-2** : Trait `QueueTransportControl` avec `play_from_queue` par défaut

### Phase 3 — Simplifier les algorithmes (2-4 semaines)
- [ ] **ALGO-2** : Macro `dispatch!` pour `MusicRendererBackend`
- [ ] **ALGO-3** : Centraliser la protection des durées de stream
- [ ] **ALGO-4** : Unifier la détection de flux continu
- [ ] **ALGO-1** : Refactoriser `schedule_sync` (extraire `sync_worker_loop`)

### Phase 4 — Qualité continue
- [ ] **BP-1** : Remplacer les `.unwrap()` critiques
- [ ] **BP-2** : Gestion d'erreur dans les threads
- [ ] **BP-3** : Visibilité des champs `PlaylistBinding`
- [ ] **BP-4** : Documentation des traits
