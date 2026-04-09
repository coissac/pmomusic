** Ce travail devra être réalisé en suivant scrupuleusement les consignes listées dans le fichier [@Rules_optimal.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules_optimal.md) **

# Async Queue Refresh — Étape 3

**Contexte**: `refresh_attached_queue_for()` dans `control_point.rs` est appelé de 3 endroits
et bloque son thread pendant toute la synchronisation (browse media server + 100+ opérations
SOAP/mémoire). L'objectif est de factoriser le mécanisme async au niveau de la couche queue
(`MusicQueue`), qui est déjà l'abstraction agnostique du backend. `control_point.rs` ne doit
plus connaître les threads ni les tokens d'annulation.

**Principe architectural**: la couche queue sait *comment* syncer (mémoire ou SOAP) et donc
aussi *comment* annuler et *quand* signaler que la lecture peut démarrer. `control_point.rs`
sait seulement *quoi* syncer (browse + conversion PlaybackItem). Les deux responsabilités
restent séparées.

---

## Vue d'ensemble des changements

```
AVANT
  control_point.rs
    refresh_attached_queue_for()
      → browse()
      → sync_queue(items)   ← bloquant 1-5s

APRÈS
  control_point.rs
    do_queue_refresh_work()  ← interne, fait le browse + conversion
  MusicQueue (couche queue)
    schedule_sync(items, callbacks)  ← non-bloquant, retourne immédiatement
      → thread "queue-sync-{renderer_id}"
           → QueueBackend::sync_queue(items, cancel_token, on_ready)
```

---

## Fichiers à modifier / créer

| Fichier | Action |
|---------|--------|
| `pmocontrol/src/errors.rs` | Ajouter variante `SyncCancelled` |
| `pmocontrol/src/queue/backend.rs` | Modifier signature `sync_queue()` |
| `pmocontrol/src/queue/interne.rs` | Adapter signature `sync_queue()` |
| `pmocontrol/src/queue/openhome.rs` | Adapter + points de vérification cancel + on_ready |
| `pmocontrol/src/queue/music_queue.rs` | Ajouter champs async + méthode `schedule_sync()` |
| `pmocontrol/src/queue/mod.rs` | Exporter `SyncScheduleOutcome` |
| `pmocontrol/src/model.rs` | Ajouter événements `QueueReadyToPlay`, `QueueSyncCancelled` |
| `pmocontrol/src/sse.rs` | Sérialiser les deux nouveaux événements |
| `pmocontrol/src/control_point.rs` | Remplacer les 3 call sites bloquants |

---

## Étape 1 — Nouvelle variante d'erreur (`errors.rs`)

**Fichier**: `pmocontrol/src/errors.rs`

Ajouter après la ligne 51 (`ControlPoint`) :

```rust
#[error("Queue sync cancelled (superseded by a newer request)")]
SyncCancelled,
```

Cette variante est retournée par `sync_queue()` quand le `cancel_token` passe à `true`.
Elle est **non-fatale** — le coordinator la traite comme un comportement normal, pas une
erreur à logger en `warn!`.

---

## Étape 2 — Modifier la signature de `sync_queue()` dans le trait (`backend.rs`)

**Fichier**: `pmocontrol/src/queue/backend.rs`, ligne 110

```rust
// AVANT
fn sync_queue(&mut self, items: Vec<PlaybackItem>) -> Result<(), ControlPointError>;

// APRÈS
use std::sync::{Arc, atomic::AtomicBool};

fn sync_queue(
    &mut self,
    items: Vec<PlaybackItem>,
    cancel_token: &Arc<AtomicBool>,
    on_ready: Option<Box<dyn FnOnce() + Send>>,
) -> Result<(), ControlPointError>;
```

**Sémantique des paramètres** :
- `cancel_token` : si `true` au moment d'une opération, retourner `Err(SyncCancelled)` immédiatement
- `on_ready` : callback one-shot appelé quand la lecture peut démarrer (voir logique ci-dessous)

---

## Étape 3 — Adapter `InternalQueue::sync_queue()` (`interne.rs`)

**Fichier**: `pmocontrol/src/queue/interne.rs`

Trouver la méthode `sync_queue()` et adapter la signature. Le corps reste identique,
avec deux ajouts :

**1. Vérification du pivot (early start)** : si une piste est en cours de lecture
(un `current_index` est défini dans le snapshot courant), appeler `on_ready` immédiatement
avant toute opération — la piste courante sera préservée.

**2. Si pas de pivot** (queue vide ou aucune piste en cours) : appeler `on_ready` après
avoir inséré le premier item.

**3. Vérification cancel** : après chaque item inséré/supprimé (en pratique `InternalQueue`
est rapide mais le principe doit être cohérent) :

```rust
fn sync_queue(
    &mut self,
    items: Vec<PlaybackItem>,
    cancel_token: &Arc<AtomicBool>,
    mut on_ready: Option<Box<dyn FnOnce() + Send>>,
) -> Result<(), ControlPointError> {
    use std::sync::atomic::Ordering::SeqCst;

    // Early start si pivot présent
    let has_current = self.current_index()?.is_some();
    if has_current {
        if let Some(f) = on_ready.take() { f(); }
    }

    // ... logique existante de sync_queue() ...
    // Dans la boucle d'insertions, après le 1er insert :
    if on_ready.is_some() {
        if let Some(f) = on_ready.take() { f(); }
    }
    // Après chaque opération :
    if cancel_token.load(SeqCst) {
        return Err(ControlPointError::SyncCancelled);
    }

    Ok(())
}
```

---

## Étape 4 — Adapter `OpenHomeQueue::sync_queue()` (`openhome.rs`)

**Fichier**: `pmocontrol/src/queue/openhome.rs`

### 4.1 Signature (ligne ~1277)

```rust
fn sync_queue(
    &mut self,
    items: Vec<PlaybackItem>,
    cancel_token: &Arc<AtomicBool>,
    mut on_ready: Option<Box<dyn FnOnce() + Send>>,
) -> Result<(), ControlPointError>
```

### 4.2 Early start — logique pivot

Au début de `sync_queue()`, **avant** toute opération SOAP, détecter si un pivot est présent :

```rust
// Après la récupération du snapshot (ligne ~1323), avant les branches if/else :
let has_pivot = playing_info.is_some();
if has_pivot {
    // Le pivot sera préservé — on peut démarrer la lecture immédiatement
    if let Some(f) = on_ready.take() { f(); }
}
```

Si pas de pivot (nouvelle playlist via `delete_all` + inserts depuis 0) : appeler `on_ready`
après le **1er insert réussi** dans `replace_queue()` et dans `replace_queue_standard_lcs()`.

### 4.3 Points de vérification cancel

Ajouter `if cancel_token.load(SeqCst) { return Err(SyncCancelled); }` aux endroits suivants :

- Dans `delete_marked_items()` (ligne ~458) : après chaque `delete_id_if_exists()`
- Dans `rebuild_playlist_section()` (ligne ~494) : après chaque `insert()`
- Dans `replace_queue_preserve_current()` (ligne ~419) : après chaque `delete_id_if_exists()` et `insert()`
- Dans `replace_queue_standard_lcs()` (ligne ~693-745) : après chaque delete et insert
- Dans `replace_queue()` (ligne ~1118) : après chaque insert dans la boucle
- Dans le fast path `AppendOnly` (ligne ~1318) : après chaque insert
- Dans le fast path `DeleteFromEnd` (ligne ~1336) : après chaque delete

### 4.4 Propagation du cancel_token aux helpers

Les méthodes helper privées qui font des boucles doivent recevoir le token :

```rust
fn delete_marked_items(
    &mut self,
    old_ids: &[u32],
    keep_flags: &[bool],
    position_label: &str,
    cancel_token: &Arc<AtomicBool>,
) -> Result<(), ControlPointError>

fn rebuild_playlist_section(
    &mut self,
    // ... params existants ...
    cancel_token: &Arc<AtomicBool>,
    on_ready: &mut Option<Box<dyn FnOnce() + Send>>,
) -> Result<u32, ControlPointError>
```

---

## Étape 5 — Adapter `MusicQueue` (dispatch enum) (`music_queue.rs`)

**Fichier**: `pmocontrol/src/queue/music_queue.rs`

### 5.1 Adapter le dispatch `sync_queue()` (ligne ~98)

```rust
fn sync_queue(
    &mut self,
    items: Vec<PlaybackItem>,
    cancel_token: &Arc<AtomicBool>,
    on_ready: Option<Box<dyn FnOnce() + Send>>,
) -> Result<(), ControlPointError> {
    match self {
        MusicQueue::Internal(q) => q.sync_queue(items, cancel_token, on_ready),
        MusicQueue::OpenHome(q) => q.sync_queue(items, cancel_token, on_ready),
    }
}
```

### 5.2 Ajouter l'état async dans `MusicQueue`

`MusicQueue` passe de simple enum de dispatch à une struct qui **encapsule** l'enum backend
et l'état de synchronisation async :

```rust
// AVANT
pub enum MusicQueue {
    Internal(InternalQueue),
    OpenHome(OpenHomeQueue),
}

// APRÈS
pub struct MusicQueue {
    backend: MusicQueueBackend,
    // État async de synchronisation
    sync_in_progress: Arc<AtomicBool>,
    sync_pending: Arc<AtomicBool>,
    sync_cancel_token: Arc<AtomicBool>,
}

// L'enum devient privée
enum MusicQueueBackend {
    Internal(InternalQueue),
    OpenHome(OpenHomeQueue),
}
```

**Note**: le changement de `enum` en `struct` implique de mettre à jour toutes les
utilisations de `MusicQueue::Internal(...)` et `MusicQueue::OpenHome(...)` dans le reste
du code (essentiellement `music_queue.rs` lui-même et `mod.rs`). Les callers externes
utilisent `MusicQueue` via `QueueBackend` et `QueueFromRendererInfo` — ils ne sont pas
impactés si l'API publique est préservée.

### 5.3 Enum résultat et méthode `schedule_sync()`

```rust
pub enum SyncScheduleOutcome {
    /// Thread spawné, sync en cours.
    Scheduled,
    /// Sync déjà en cours — annulée et nouvelle sync programmée en pending.
    AlreadyRunning,
}
```

```rust
impl MusicQueue {
    /// Lance une synchronisation asynchrone de la queue.
    ///
    /// - Si aucune sync n'est en cours : spawne un thread, retourne `Scheduled`.
    /// - Si une sync est en cours : l'annule, note une sync pending, retourne `AlreadyRunning`.
    ///   Le thread en cours finira l'opération courante, détectera le cancel, puis
    ///   relancera la sync avec les nouveaux items via `pending_items_fn`.
    ///
    /// `pending_items_fn` : closure appelée dans le worker pour re-fetcher les items
    ///   en cas de pending. Elle doit être Send + 'static car elle s'exécute dans un thread.
    ///
    /// `on_ready` : appelé dès que la lecture peut démarrer (pivot préservé ou 1er insert).
    ///
    /// `renderer_id` : utilisé uniquement pour nommer le thread de travail.
    pub fn schedule_sync(
        &self,  // &self car l'état async est derrière Arc<AtomicBool>
        renderer_id: &str,
        items: Vec<PlaybackItem>,
        pending_items_fn: Box<dyn Fn() -> Result<Vec<PlaybackItem>, ControlPointError> + Send + 'static>,
        on_ready: Option<Box<dyn FnOnce() + Send + 'static>>,
    ) -> SyncScheduleOutcome
```

**Problème de `&mut self` vs `&self`** : `sync_queue()` dans le trait prend `&mut self`
car les backends mutent leur état. Mais `schedule_sync()` veut spawner un thread qui détient
le backend. Solution : le backend est déjà derrière `Arc<Mutex<MusicQueue>>` dans
`MusicRenderer` (ligne 124 de musicrenderer.rs — c'est le `queue` field). Le thread worker
clone cet `Arc` et acquiert le lock pour appeler `sync_queue()`.

**Signature révisée** :

```rust
/// Doit être appelé avec un Arc<Mutex<Self>> pour permettre le spawn du thread worker.
pub fn schedule_sync(
    queue_arc: &Arc<Mutex<MusicQueue>>,
    renderer_id: &str,
    items: Vec<PlaybackItem>,
    pending_items_fn: Box<dyn Fn() -> Result<Vec<PlaybackItem>, ControlPointError> + Send + 'static>,
    on_ready: Option<Box<dyn FnOnce() + Send + 'static>>,
) -> SyncScheduleOutcome {
    use std::sync::atomic::Ordering::SeqCst;

    let (sync_in_progress, sync_pending, sync_cancel_token) = {
        let q = queue_arc.lock().unwrap();
        (
            Arc::clone(&q.sync_in_progress),
            Arc::clone(&q.sync_pending),
            Arc::clone(&q.sync_cancel_token),
        )
    };

    if sync_in_progress.swap(true, SeqCst) {
        // Sync en cours : annuler et noter pending
        sync_cancel_token.store(true, SeqCst);
        sync_pending.store(true, SeqCst);
        return SyncScheduleOutcome::AlreadyRunning;
    }

    // Pas de sync en cours : initialiser et spawner
    sync_cancel_token.store(false, SeqCst);
    sync_pending.store(false, SeqCst);

    let queue_arc = Arc::clone(queue_arc);
    let thread_name = format!("queue-sync-{}", renderer_id);

    thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            // Guard: libère in_progress à la sortie même en cas de panic
            struct Guard(Arc<AtomicBool>);
            impl Drop for Guard {
                fn drop(&mut self) { self.0.store(false, SeqCst); }
            }
            let _guard = Guard(Arc::clone(&sync_in_progress));

            let mut current_items = items;
            let mut current_on_ready = Some(on_ready);

            loop {
                sync_pending.store(false, SeqCst);
                sync_cancel_token.store(false, SeqCst);

                let result = {
                    let mut q = queue_arc.lock().unwrap();
                    q.backend.sync_queue(
                        current_items,
                        &sync_cancel_token,
                        current_on_ready.take().flatten(),
                    )
                };

                match result {
                    Err(ControlPointError::SyncCancelled) => {
                        // Annulé normalement — vérifier si pending
                    }
                    Err(e) => {
                        warn!("queue-sync error: {}", e);
                    }
                    Ok(()) => {}
                }

                if !sync_pending.load(SeqCst) {
                    break; // Pas de nouvelle sync en attente → terminer
                }

                // Nouvelle sync demandée pendant l'exécution → re-fetcher et relancer
                match pending_items_fn() {
                    Ok(new_items) => {
                        current_items = new_items;
                        current_on_ready = Some(None); // pas de on_ready pour les re-syncs
                    }
                    Err(e) => {
                        warn!("queue-sync pending re-fetch error: {}", e);
                        break;
                    }
                }
            }
            // _guard libère sync_in_progress = false
        })
        .expect("Failed to spawn queue-sync thread");

    SyncScheduleOutcome::Scheduled
}
```

---

## Étape 6 — Nouveaux événements SSE

### 6.1 `model.rs`

Localiser l'enum `RendererEvent` et ajouter :

```rust
/// Émis dès que la queue peut être lue (pivot préservé ou 1er track inséré).
QueueReadyToPlay {
    id: DeviceId,
},
/// Émis quand une sync est annulée car une nouvelle a été demandée.
QueueSyncCancelled {
    id: DeviceId,
},
```

### 6.2 `sse.rs`

Localiser le match de sérialisation des `RendererEvent` et ajouter les deux variantes.
Suivre le pattern existant de `QueueRefreshing` (ligne ~152) :

```rust
RendererEvent::QueueReadyToPlay { id } => {
    // sérialiser avec type = "queue_ready_to_play"
}
RendererEvent::QueueSyncCancelled { id } => {
    // sérialiser avec type = "queue_sync_cancelled"
}
```

---

## Étape 7 — Modifier `control_point.rs`

### 7.1 Extraire la logique de browse

Renommer `refresh_attached_queue_for()` en deux fonctions :

**`fetch_queue_items_for()`** (nouvelle, interne) : fait le browse + conversion, retourne
`Vec<PlaybackItem>`. C'est la `pending_items_fn` passée à `schedule_sync()`.

**`schedule_queue_refresh_for()`** (remplace l'ancienne) : appelle `fetch_queue_items_for()`,
puis `MusicQueue::schedule_sync()`.

```rust
fn fetch_queue_items_for(
    registry: &Arc<RwLock<DeviceRegistry>>,
    renderer_id: &DeviceId,
) -> Result<Vec<PlaybackItem>, ControlPointError> {
    // Browse media server + conversion PlaybackItem
    // (logique actuellement dans refresh_attached_queue_for() lignes ~1593-1670)
}

fn schedule_queue_refresh_for(
    registry: &Arc<RwLock<DeviceRegistry>>,
    renderer_id: &DeviceId,
    event_bus: &RendererEventBus,
    auto_play_cb: Option<Box<dyn FnOnce(&DeviceId) -> Result<(), ControlPointError> + Send + 'static>>,
) -> SyncScheduleOutcome {
    let items = match fetch_queue_items_for(registry, renderer_id) {
        Ok(items) => items,
        Err(e) => { warn!(...); return SyncScheduleOutcome::Scheduled; /* ou erreur */ }
    };

    // on_ready : déclenche auto_play si demandé + émet QueueReadyToPlay SSE
    let rid = renderer_id.clone();
    let bus = event_bus.clone();
    let on_ready: Option<Box<dyn FnOnce() + Send + 'static>> = Some(Box::new(move || {
        bus.broadcast(RendererEvent::QueueReadyToPlay { id: rid.clone() });
        if let Some(cb) = auto_play_cb {
            if let Err(e) = cb(&rid) {
                warn!("auto-play callback failed: {}", e);
            }
        }
    }));

    // pending_items_fn : re-fetcher depuis le media server si pending
    let registry2 = Arc::clone(registry);
    let rid2 = renderer_id.clone();
    let pending_fn = Box::new(move || fetch_queue_items_for(&registry2, &rid2));

    // Récupérer l'Arc<Mutex<MusicQueue>> du renderer depuis le registry
    let queue_arc = {
        let reg = registry.read().unwrap();
        reg.get_renderer_queue_arc(renderer_id)? // méthode à ajouter dans DeviceRegistry
    };

    // Émettre QueueRefreshing avant de lancer
    event_bus.broadcast(RendererEvent::QueueRefreshing { id: renderer_id.clone() });

    let outcome = MusicQueue::schedule_sync(&queue_arc, &renderer_id.0, items, pending_fn, on_ready);

    if matches!(outcome, SyncScheduleOutcome::AlreadyRunning) {
        event_bus.broadcast(RendererEvent::QueueSyncCancelled { id: renderer_id.clone() });
    }

    outcome
}
```

### 7.2 Call site 1 — thread "cp-media-server-event-worker" (l.270)

```rust
// AVANT
let _ = refresh_attached_queue_for(&registry, &renderer_id, &event_bus, None);
// APRÈS
schedule_queue_refresh_for(&registry, &renderer_id, &event_bus, None);
```

### 7.3 Call site 2 — thread "cp-playlist-periodic-refresh" (l.340)

```rust
// AVANT
let _ = refresh_attached_queue_for(&registry_for_periodic, &renderer_id, &event_bus_for_periodic, None);
// APRÈS
schedule_queue_refresh_for(&registry_for_periodic, &renderer_id, &event_bus_for_periodic, None);
```

### 7.4 Call site 3 — `attach_queue_to_playlist_async()` (l.1264)

```rust
pub async fn attach_queue_to_playlist_async(
    &self, renderer_id: &DeviceId, server_id: &DeviceId, container_id: &str, auto_play: bool,
) -> Result<(), ControlPointError> {
    // 1. Enregistrer la liaison (synchrone, ~1ms)
    self.registry.write().unwrap()
        .set_playlist_binding(renderer_id, server_id, container_id);

    // 2. Construire le callback auto-play si demandé
    let cp = self.clone();
    let rid = renderer_id.clone();
    let auto_play_cb: Option<Box<dyn FnOnce(&DeviceId) -> Result<(), ControlPointError> + Send + 'static>> =
        if auto_play {
            Some(Box::new(move |id| cp.play_current_from_queue(id)))
        } else {
            None
        };

    // 3. Lancer le refresh async — retourne immédiatement
    schedule_queue_refresh_for(&self.registry, renderer_id, &self.event_bus, auto_play_cb);

    Ok(())  // La webapp sera notifiée via SSE (QueueRefreshing → QueueReadyToPlay → QueueUpdated)
}
```

### 7.5 Fin de sync — émettre `QueueUpdated`

L'événement `QueueUpdated` (avec `queue_length`) est actuellement émis à la ligne ~1718
dans `refresh_attached_queue_for()`. Il doit être émis à la fin du worker thread dans
`MusicQueue::schedule_sync()`, après le `sync_queue()` réussi.

Passer un `on_complete` callback à `schedule_sync()` (en plus de `on_ready`) :

```rust
pub fn schedule_sync(
    queue_arc: &Arc<Mutex<MusicQueue>>,
    renderer_id: &str,
    items: Vec<PlaybackItem>,
    pending_items_fn: Box<dyn Fn() -> Result<Vec<PlaybackItem>, ControlPointError> + Send + 'static>,
    on_ready: Option<Box<dyn FnOnce() + Send + 'static>>,
    on_complete: Box<dyn Fn(usize) + Send + 'static>,  // NOUVEAU — reçoit queue_length
) -> SyncScheduleOutcome
```

Dans le worker, après `Ok(())` du `sync_queue()` :

```rust
Ok(()) => {
    let queue_len = queue_arc.lock().unwrap().len().unwrap_or(0);
    on_complete(queue_len);
}
```

Dans `schedule_queue_refresh_for()` :

```rust
let bus3 = event_bus.clone();
let rid3 = renderer_id.clone();
let on_complete = Box::new(move |queue_len: usize| {
    bus3.broadcast(RendererEvent::QueueUpdated {
        id: rid3.clone(),
        queue_length: queue_len,
    });
});
```

---

## Étape 8 — Accès à `Arc<Mutex<MusicQueue>>` depuis le registry

`schedule_queue_refresh_for()` a besoin d'accéder à l'`Arc<Mutex<MusicQueue>>` du renderer.
Localiser dans `DeviceRegistry` comment les renderers et leurs queues sont stockés, et ajouter
une méthode :

```rust
pub fn get_renderer_queue_arc(
    &self,
    renderer_id: &DeviceId,
) -> Result<Arc<Mutex<MusicQueue>>, ControlPointError>
```

(Ou équivalent selon la structure réelle du registry.)

---

## Ordre d'implémentation

```
Étape 1 (errors.rs)          ← 5 min
Étape 2 (backend.rs)         ← 10 min, casse la compilation → à faire avant les autres
Étape 3 (interne.rs)         ← 30 min
Étape 4 (openhome.rs)        ← 1-2h (nombreux points de vérification cancel)
Étape 5 (music_queue.rs)     ← 2-3h (changement struct + schedule_sync)
Étape 6 (model.rs + sse.rs)  ← 30 min
Étape 7 (control_point.rs)   ← 1h
Étape 8 (registry)           ← 30 min selon la structure
```

Après l'étape 2, `cargo build` cassera jusqu'à l'étape 5 incluse — c'est attendu.
Faire les étapes 3, 4, 5 dans la même session sans interrompre.

## Tests

```bash
cargo build -p pmocontrol

# Vérifier les scénarios :
# 1. Attach playlist → QueueRefreshing SSE immédiat, QueueReadyToPlay après 1er insert,
#    QueueUpdated après fin complète
# 2. Attach 2e playlist pendant sync en cours → QueueSyncCancelled + nouveau refresh repart
# 3. Piste en cours de lecture pendant sync → QueueReadyToPlay immédiat (pivot préservé)
# 4. Refresh périodique (60s) ne bloque plus son thread

RUST_LOG=debug cargo run 2>&1 | grep -E "queue.sync|SyncCancelled|Scheduled|AlreadyRunning|on_ready|on_complete"
```

---

*Date: 2026-04-09*
