** Ce travail devra être réalisé en suivant scrupuleusement les consignes listées dans le fichier [@Rules_optimal.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules_optimal.md) **

## Contexte

Ce plan fait suite à la revue de code de `pmowebrenderer` et à l'analyse d'écart avec la vision
décrite dans `Blackboard/ToThinkAbout/webrenderer.md`. La crate est fonctionnelle pour le cas
browser simple, mais son architecture actuelle empêche l'ajout de nouveaux types de devices
(Android Auto, Chromecast, Sonos…) sans réécriture. Ce plan prépare ces extensions sans les
implémenter, tout en finissant correctement le player browser.

**Périmètre** : uniquement `pmowebrenderer/` et `pmoapp/webapp/src/services/PMOPlayer.ts`.
Aucune modification aux autres crates (`pmoupnp`, `pmoaudio_ext`, `pmocontrol`).

---

## Problèmes à résoudre

### P0 — Bug résiduel : `play_handler` pose `Transitioning` même sans URI

`handlers.rs:23` : quand `has_uri = false`, `playback_state` est mis à `Transitioning` mais le
pipeline ne joue rien — l'état reste bloqué indéfiniment.

### P1 — `RendererRegistry` mélange gestion de cycle de vie et livraison browser-spécifique

Les méthodes `set_player_command`, `get_pending_command`, `has_current_uri`,
`send_play_command`, `send_pause_command` sont hardcodées pour le mécanisme HTTP-polling du
browser. Pour Android Auto, on devrait dupliquer la registry entière ou la modifier.

### P2 — Canal de commandes browser non typé et à slot unique

`player_command: Option<serde_json::Value>` dans `RendererState` :
- une typo dans le type JSON passe sans erreur de compilation
- une seule commande peut être en attente : `flush` suivi de `stream` écrase `flush`

### P3 — `OggFlacStreamHandle.pause()/resume()` jamais appelé

Quand UPnP `Pause` est reçu, `pipeline.send(Pause)` arrête le décodeur mais le flux HTTP
continue à servir les bytes déjà encodés. `flac_handle.pause()` (qui envoie du silence pour les
flux continus) n'est jamais appelé. La pause n'est donc pas transmise au navigateur via le flux.

### P4 — La commande `flush` n'est jamais envoyée au browser

`handleCommand('flush')` existe dans `PMOPlayer.ts` mais aucun code backend ne l'envoie.
Lors d'un Stop ou d'un changement de piste, le browser a potentiellement plusieurs secondes
d'audio bufférisé non vidé. Sans `flush`, les transitions de piste ont un délai de 3–5 secondes.

### P5 — `AudioContext` absent dans `PMOPlayer.ts`

Le document spécifie `audioContext.suspend()` comme solution au buffer ~5s. L'`AudioContext`
était déclaré mais non instancié (code mort), et a été supprimé. Il faut le réintroduire
correctement : instancié, connecté à `<audio>` via `createMediaElementSource()`, et utilisé
dans `flush()` pour vider le buffer audio decoded.

### P6 — Pas de reconnexion automatique sur coupure du stream HTTP

Si le flux OGG-FLAC est interrompu (redémarrage serveur, coupure réseau), `PMOPlayer.ts` entre
en état d'erreur sans tenter de se reconnecter. Pour un player destiné à rester actif longtemps,
c'est un manque critique.

### P7 — Format de position incohérent entre les deux chemins d'écriture

`run_event_listener` (pipeline events) écrit `seconds_to_upnp_time(pos)` → `"1:23:45"`.
`update_player_state` (rapports HTTP du browser) écrit `pos.to_string()` → `"83.5"`.
`GetPositionInfo` retourne le dernier qui a écrit — parfois un float, parfois du HH:MM:SS.

### P8 — Aucun endpoint de métadonnées JSON

Pas de `/nowplaying`, `/metadata`, ni `/state`. Une app mobile qui veut afficher titre/artiste/
pochette et l'état de lecture ne peut pas accéder à ces données sans passer par les actions SOAP
UPnP. Ces endpoints sont la fondation commune à tous les futurs adaptateurs.

---

## Solution globale

### Architecture cible de `pmowebrenderer/src/`

```
pmowebrenderer/src/
  core/
    mod.rs          ← re-exports publics du core
    adapter.rs      ← trait DeviceAdapter + enum DeviceCommand
    handlers.rs     ← handlers UPnP (inchangés sauf P0)
    pipeline.rs     ← pipeline audio (inchangé)
    renderer.rs     ← factory UPnP (inchangée)
    registry.rs     ← cycle de vie uniquement, sans méthodes browser-spécifiques
    state.rs        ← RendererState avec VecDeque<DeviceCommand>
    messages.rs     ← PlaybackState
    error.rs        ← WebRendererError
    config.rs       ← config extension (pmoserver)
  browser/
    mod.rs          ← module browser : register, stream, command polling
    register.rs     ← handlers HTTP register/unregister/play/pause/etc.
    stream.rs       ← endpoint GET /stream
    adapter.rs      ← impl DeviceAdapter for BrowserAdapter
  lib.rs            ← point d'entrée, routage Axum
```

Le trait `DeviceAdapter` isole tout ce qui est device-spécifique. Ajouter Android Auto = créer
`android/adapter.rs` implémentant ce trait, sans toucher au core.

---

## Plan d'exécution

### Phase 0 — Bug résiduel P0 (1 fichier)

**`pmowebrenderer/src/core/handlers.rs`** (ou `handlers.rs` actuel avant restructure)

Dans `play_handler`, conditionner le changement d'état à `has_uri` :

```rust
pub fn play_handler(pipeline: PipelineHandle, state: SharedState, instance_id: String) -> ActionHandler {
    action_handler!(captures(pipeline, state, instance_id) |data| {
        let has_uri = state.read().current_uri.is_some();
        if !has_uri {
            // UPnP Play sans URI chargée : no-op silencieux.
            // Ne pas changer playback_state — rester dans l'état précédent.
            tracing::warn!("[WebRenderer] UPnP Play ignored: no URI loaded");
            return Ok(data);
        }
        {
            let mut s = state.write();
            s.playback_state = PlaybackState::Transitioning;
            s.push_command(DeviceCommand::Stream {
                url: format!("/api/webrenderer/{}/stream", instance_id),
            });
        }
        pipeline.send(PipelineControl::Play).await;
        Ok(data)
    })
}
```

`push_command` est définie en Phase 2 sur `RendererState`.

---

### Phase 1 — Trait `DeviceAdapter` et `DeviceCommand` typé (P1 + P2)

#### Étape 1.1 — `DeviceCommand` enum dans `core/adapter.rs`

```rust
/// Commandes envoyées au device physique (browser, Android, Cast…).
/// Typé à la compilation — pas de serde_json::Value.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeviceCommand {
    /// Charge et démarre le flux audio.
    Stream { url: String },
    /// Reprend la lecture après pause (sans changer de flux).
    Play,
    /// Met en pause.
    Pause,
    /// Seek vers timestamp_sec dans le flux.
    Seek { position_sec: f64 },
    /// Vide le buffer immédiatement (transition de piste).
    Flush,
    /// Arrêt total.
    Stop,
}
```

#### Étape 1.2 — Trait `DeviceAdapter` dans `core/adapter.rs`

```rust
/// Abstraction de livraison de commandes vers un device physique.
/// Chaque type de device implémente ce trait.
///
/// Send + Sync requis : l'adapter est partagé entre threads Tokio.
pub trait DeviceAdapter: Send + Sync + 'static {
    /// Envoie une commande au device. Fire-and-forget.
    fn deliver(&self, command: DeviceCommand);

    /// Rapporte l'état courant du device (position, durée, ready_state…).
    /// Appelé périodiquement pour synchroniser `RendererState`.
    /// Retourne None si le device n'a pas de nouvelles données.
    fn poll_state(&self) -> Option<DeviceStateReport>;
}

/// Rapport d'état du device physique → RendererState
pub struct DeviceStateReport {
    pub position_sec: Option<f64>,
    pub duration_sec: Option<f64>,
    pub playback_state: Option<DevicePlaybackState>,
}

/// État de lecture tel que rapporté par le device (distinct de PlaybackState UPnP)
pub enum DevicePlaybackState {
    Playing,
    Paused,
    Stopped,
    Buffering,
}
```

#### Étape 1.3 — `VecDeque<DeviceCommand>` dans `RendererState` (`core/state.rs`)

Remplacer `player_command: Option<serde_json::Value>` :

```rust
use std::collections::VecDeque;
use crate::core::adapter::DeviceCommand;

pub struct RendererState {
    pub playback_state: PlaybackState,
    pub current_uri: Option<String>,
    pub current_metadata: Option<String>,
    pub next_uri: Option<String>,
    pub next_metadata: Option<String>,
    pub position: Option<String>,      // format UPnP HH:MM:SS (unifié Phase 4)
    pub duration: Option<String>,      // format UPnP HH:MM:SS
    pub volume: u16,
    pub mute: bool,
    /// File de commandes en attente pour le device physique.
    /// Plusieurs commandes peuvent s'accumuler (ex: Flush puis Stream).
    pub pending_commands: VecDeque<DeviceCommand>,
}

impl RendererState {
    /// Ajoute une commande en fin de file.
    pub fn push_command(&mut self, cmd: DeviceCommand) {
        self.pending_commands.push_back(cmd);
    }

    /// Retire et retourne la commande suivante.
    pub fn pop_command(&mut self) -> Option<DeviceCommand> {
        self.pending_commands.pop_front()
    }
}
```

Mettre à jour `Default` : `pending_commands: VecDeque::new()`.

#### Étape 1.4 — `BrowserAdapter` dans `browser/adapter.rs`

Le `BrowserAdapter` implémente `DeviceAdapter` pour le mécanisme HTTP-polling actuel.
Il remplace les méthodes browser-spécifiques de `RendererRegistry`.

```rust
/// Adapter browser : commandes déposées dans RendererState.pending_commands,
/// consommées via GET /api/webrenderer/{id}/command.
pub struct BrowserAdapter {
    pub state: SharedState,
}

impl DeviceAdapter for BrowserAdapter {
    fn deliver(&self, command: DeviceCommand) {
        self.state.write().push_command(command);
    }

    fn poll_state(&self) -> Option<DeviceStateReport> {
        // Le browser reporte via POST /report — état déjà dans RendererState.
        // poll_state() non utilisé pour le browser (push-only depuis le browser).
        None
    }
}
```

#### Étape 1.5 — `WebRendererInstance` : ajouter `adapter: Arc<dyn DeviceAdapter>`

```rust
pub struct WebRendererInstance {
    pub instance_id: String,
    pub udn: String,
    pub device_instance: Arc<DeviceInstance>,
    pub state: SharedState,
    pub flac_handle: pmoaudio_ext::sinks::OggFlacStreamHandle,
    pub pipeline: PipelineHandle,
    pub created_at: SystemTime,
    /// Adapter device-spécifique pour la livraison des commandes.
    pub adapter: Arc<dyn DeviceAdapter>,
}
```

Dans `create_instance()` de `RendererRegistry`, construire et stocker un `BrowserAdapter` :

```rust
let adapter = Arc::new(BrowserAdapter { state: state.clone() });
// ...
Ok(WebRendererInstance { ..., adapter })
```

#### Étape 1.6 — Nettoyer `RendererRegistry` des méthodes browser-spécifiques

Supprimer de `RendererRegistry` :
- `set_player_command` → remplacé par `adapter.deliver(cmd)`
- `get_pending_command` → déplacé dans `browser/register.rs`
- `has_current_uri` → déplacé dans `browser/register.rs`
- `send_play_command` → inutile (on passe par le pipeline directement)
- `send_pause_command` → inutile

Les appels correspondants dans `browser/register.rs` passent désormais par :

```rust
// Récupérer l'adapter et le caster en BrowserAdapter
let instance = registry.get_instance(instance_id)?;
// Déposer la commande via l'interface générique
instance.adapter.deliver(DeviceCommand::Stream { url });
```

`get_pending_command` dans `browser/register.rs` lit directement `state.write().pop_command()`
puis le sérialise en JSON pour la réponse HTTP.

---

### Phase 2 — Intégration `OggFlacStreamHandle` (P3 + P4)

#### Étape 2.1 — Exposer `flac_handle` dans `PipelineHandle` (`core/pipeline.rs`)

Ajouter `flac_handle: OggFlacStreamHandle` à `PipelineHandle` (il est déjà dans
`InstancePipeline` mais pas dans le handle exposé aux handlers) :

```rust
#[derive(Clone)]
pub struct PipelineHandle {
    pub player: PlayerHandle,
    pub stop_token: CancellationToken,
    pub flac_handle: OggFlacStreamHandle,
    state: SharedState,
}
```

Mettre à jour la construction dans `InstancePipeline::start()` :

```rust
let pipeline_handle = PipelineHandle {
    player: player_handle,
    stop_token: stop_token.clone(),
    flac_handle: flac_handle.clone(),   // ← ajouter
    state,
};
```

#### Étape 2.2 — `pause_handler` UPnP : appeler `flac_handle.pause()` (`core/handlers.rs`)

```rust
pub fn pause_handler(pipeline: PipelineHandle, state: SharedState, adapter: Arc<dyn DeviceAdapter>) -> ActionHandler {
    action_handler!(captures(pipeline, state, adapter) |data| {
        pipeline.send(PipelineControl::Pause).await;
        pipeline.flac_handle.pause();   // envoie silence au lieu de backpressure
        adapter.deliver(DeviceCommand::Pause);
        state.write().playback_state = PlaybackState::Paused;
        Ok(data)
    })
}
```

Note : `flac_handle.pause()` est synchrone (atomic store) — pas d'await nécessaire.

#### Étape 2.3 — `stop_handler` UPnP : envoyer `Flush` puis `Stop` au device

```rust
pub fn stop_handler(pipeline: PipelineHandle, state: SharedState, adapter: Arc<dyn DeviceAdapter>) -> ActionHandler {
    action_handler!(captures(pipeline, state, adapter) |data| {
        pipeline.send(PipelineControl::Stop).await;
        pipeline.flac_handle.pause();   // coupe le flux
        // Flush d'abord pour vider le buffer du device, puis Stop.
        adapter.deliver(DeviceCommand::Flush);
        adapter.deliver(DeviceCommand::Stop);
        state.write().playback_state = PlaybackState::Stopped;
        Ok(data)
    })
}
```

#### Étape 2.4 — `run_event_listener` : envoyer `Flush + Stream` sur `TrackEnded` (`core/pipeline.rs`)

Quand `PlayerEvent::TrackEnded` se produit, le pipeline va enchaîner sur la prochaine URI.
Le device doit vider son buffer avant de recevoir le nouveau flux.

```rust
PlayerEvent::TrackEnded => {
    state.write().playback_state = PlaybackState::Transitioning;
    // Vider le buffer du device avant la nouvelle piste.
    // Stream sera envoyé par play_handler quand le CP appelle Play sur la nouvelle URI.
    // On envoie Flush maintenant pour minimiser le délai de transition.
    if let Some(adapter) = adapter.upgrade() {
        adapter.deliver(DeviceCommand::Flush);
    }
    // ...suite pmoserver inchangée
}
```

`adapter` dans `run_event_listener` est un `Weak<dyn DeviceAdapter>` (pour éviter un cycle
de référence avec `WebRendererInstance`). Récupérer via `.upgrade()`.

Mettre à jour la signature de `run_event_listener` :

```rust
async fn run_event_listener(
    mut event_rx: broadcast::Receiver<PlayerEvent>,
    state: SharedState,
    adapter: Weak<dyn DeviceAdapter>,
    udn: String,
    #[cfg(feature = "pmoserver")]
    control_point: Arc<ControlPoint>,
)
```

Et l'appel dans `InstancePipeline::start()` :

```rust
let adapter_weak = Arc::downgrade(&instance_adapter);
tokio::spawn(async move {
    run_event_listener(event_rx, state_clone, adapter_weak, udn_clone, ...).await;
});
```

#### Étape 2.5 — `play_handler` UPnP : `flac_handle.resume()` sur reprise de lecture

```rust
pub fn play_handler(pipeline: PipelineHandle, state: SharedState, instance_id: String, adapter: Arc<dyn DeviceAdapter>) -> ActionHandler {
    action_handler!(captures(pipeline, state, instance_id, adapter) |data| {
        let has_uri = state.read().current_uri.is_some();
        if !has_uri {
            tracing::warn!("[WebRenderer] UPnP Play ignored: no URI loaded");
            return Ok(data);
        }
        pipeline.flac_handle.resume();   // annule un éventuel pause() précédent
        {
            let mut s = state.write();
            s.playback_state = PlaybackState::Transitioning;
            s.push_command(DeviceCommand::Stream {
                url: format!("/api/webrenderer/{}/stream", instance_id),
            });
        }
        pipeline.send(PipelineControl::Play).await;
        Ok(data)
    })
}
```

#### Étape 2.6 — Mettre à jour `WebRendererFactory::build_avtransport()` (`core/renderer.rs`)

`build_avtransport` reçoit maintenant `adapter: Arc<dyn DeviceAdapter>` en plus de `pipeline`
et `state` :

```rust
fn build_avtransport(
    pipeline: PipelineHandle,
    state: SharedState,
    instance_id: &str,
    adapter: Arc<dyn DeviceAdapter>,
) -> Result<Service, FactoryError>
```

Propager depuis `create_device_with_pipeline` (qui reçoit déjà `device_name` = `instance_id`).
L'adapter est créé dans `create_instance()` de `RendererRegistry` avant d'appeler la factory,
puis passé à travers.

---

### Phase 3 — Browser player : finition (P5 + P6 + P7)

#### Étape 3.1 — `AudioContext` dans `PMOPlayer.ts`

L'`AudioContext` réduit le buffer décodé de ~5s à ~50ms, rendant `flush()` réellement immédiat.
Il doit être connecté à l'élément `<audio>` via `createMediaElementSource()`.

**Contrainte critique** : `createMediaElementSource()` ne peut être appelé qu'une seule fois
par élément audio. L'appel doit être différé jusqu'au premier geste utilisateur (autoplay policy).

```typescript
private ac: AudioContext | null = null;

private ensureAudioContext(): AudioContext {
    if (!this.ac) {
        this.ac = new AudioContext();
        // Connecter l'élément audio au contexte.
        // createMediaElementSource() ne peut être appelé qu'une fois.
        const source = this.ac.createMediaElementSource(this.audio);
        source.connect(this.ac.destination);
    }
    return this.ac;
}

play() {
    this.log('play()');
    const ac = this.ensureAudioContext();
    // Reprendre le contexte si suspendu (après flush)
    if (ac.state === 'suspended') {
        ac.resume().catch(err => this.log('AudioContext resume error', err));
    }
    this.audio.play().catch(err => {
        if ((err as DOMException).name === 'NotAllowedError') {
            this.pendingPlay = true;
            this.setupAutoplayUnlock();
        } else {
            this.log('play error', err);
        }
    });
}

flush() {
    this.log('flush()');
    this.audio.pause();
    this.audio.removeAttribute('src');
    this.audio.load();
    // Suspendre le contexte audio pour vider le buffer décodé (~50ms au lieu de ~5s).
    this.ac?.suspend();
    this.listeners.flush?.();
}

destroy() {
    // ... (inchangé, ac?.close() ajouté à la fin)
    this.ac?.close();
    this.ac = null;
}
```

**Attention** : après `createMediaElementSource()`, le volume de l'élément `<audio>` n'est plus
contrôlé par `.volume` mais par un `GainNode`. Si le volume est géré par le backend (RenderingControl
UPnP), ce n'est pas un problème. Sinon, ajouter un `GainNode` intermédiaire.

#### Étape 3.2 — Auto-reconnect sur coupure du stream HTTP

Ajouter un handler `error` spécifique à la coupure réseau, distinct de l'erreur de src vide
(produite par `flush()`). La coupure est identifiée par `audio.error.code === MEDIA_ERR_NETWORK`
ou `MEDIA_ERR_DECODE` alors qu'une `src` est présente.

```typescript
private reconnectAttempts = 0;
private readonly MAX_RECONNECT_ATTEMPTS = 5;
private reconnectTimeout: number | null = null;

private scheduleReconnect() {
    if (this.reconnectAttempts >= this.MAX_RECONNECT_ATTEMPTS) {
        this.log('max reconnect attempts reached, giving up');
        this.setState('error');
        return;
    }
    // Backoff exponentiel : 1s, 2s, 4s, 8s, 16s
    const delayMs = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 16000);
    this.reconnectAttempts++;
    this.log(`reconnect attempt ${this.reconnectAttempts} in ${delayMs}ms`);
    this.reconnectTimeout = window.setTimeout(() => {
        const url = this.audio.getAttribute('data-stream-url');
        if (url) {
            this.stream(url);
            this.play();
        }
    }, delayMs);
}

stream(url: string) {
    this.log('stream:', url);
    // Mémoriser l'URL pour la reconnexion.
    this.audio.setAttribute('data-stream-url', url);
    this.reconnectAttempts = 0; // reset compteur sur nouveau stream
    this.audio.src = url;
    this.audio.load();
}
```

Dans `setupAudioListeners()`, affiner le handler `error` :

```typescript
this.audio.addEventListener('error', () => {
    // Ignorer l'erreur produite par flush() (removeAttribute('src') + load())
    if (!this.audio.getAttribute('src')) return;
    const code = this.audio.error?.code;
    const isNetworkError = code === MediaError.MEDIA_ERR_NETWORK
                        || code === MediaError.MEDIA_ERR_DECODE;
    if (isNetworkError && this.state !== 'stopped') {
        this.log('network error, scheduling reconnect');
        this.scheduleReconnect();
    } else {
        this.log('unrecoverable error', this.audio.error);
        this.setState('error');
        this.listeners.error?.(this.audio.error?.message || 'unknown error');
    }
});
```

Ajouter le nettoyage dans `destroy()` :
```typescript
if (this.reconnectTimeout !== null) {
    clearTimeout(this.reconnectTimeout);
    this.reconnectTimeout = null;
}
```

#### Étape 3.3 — Unifier le format de position (`registry.rs`)

Dans `update_player_state()`, convertir `position_sec: f64` en format UPnP avant de l'écrire :

```rust
// Avant :
state.position = Some(pos.to_string());   // → "83.5"

// Après :
state.position = Some(crate::pipeline::seconds_to_upnp_time(pos));  // → "1:23:45"
```

Idem pour `duration_sec` :

```rust
state.duration = Some(crate::pipeline::seconds_to_upnp_time(dur));
```

---

### Phase 4 — Endpoints de métadonnées (P8)

Ces endpoints sont la fondation commune pour toutes les apps (mobile, multiroom…).
Ils exposent l'état de `RendererState` en JSON, sans passer par SOAP UPnP.

#### Étape 4.1 — Handler `GET /api/webrenderer/{id}/nowplaying`

Retourne les métadonnées de la piste courante et la position :

```rust
#[derive(Serialize)]
pub struct NowPlayingResponse {
    pub state: String,           // "PLAYING", "PAUSED", "STOPPED", "TRANSITIONING"
    pub current_uri: Option<String>,
    pub current_metadata: Option<String>,
    pub position: Option<String>,   // HH:MM:SS
    pub duration: Option<String>,   // HH:MM:SS
    pub volume: u16,
    pub mute: bool,
}

/// GET /api/webrenderer/{id}/nowplaying
pub async fn nowplaying_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let state = match registry.get_state(&instance_id) {
        Some(s) => s,
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let s = state.read();
    let response = NowPlayingResponse {
        state: match s.playback_state {
            PlaybackState::Playing => "PLAYING",
            PlaybackState::Paused => "PAUSED",
            PlaybackState::Stopped => "STOPPED",
            PlaybackState::Transitioning => "TRANSITIONING",
        }.to_string(),
        current_uri: s.current_uri.clone(),
        current_metadata: s.current_metadata.clone(),
        position: s.position.clone(),
        duration: s.duration.clone(),
        volume: s.volume,
        mute: s.mute,
    };
    Json(response).into_response()
}
```

Ajouter `get_state()` dans `RendererRegistry` (retourne `Option<SharedState>` par `instance_id`).

#### Étape 4.2 — Handler `GET /api/webrenderer/{id}/state`

Retourne l'état complet incluant next track, pour permettre une UI de file d'attente :

```rust
#[derive(Serialize)]
pub struct RendererStateResponse {
    pub instance_id: String,
    pub udn: String,
    // tous les champs de RendererState
    pub playback_state: String,
    pub current_uri: Option<String>,
    pub current_metadata: Option<String>,
    pub next_uri: Option<String>,
    pub next_metadata: Option<String>,
    pub position: Option<String>,
    pub duration: Option<String>,
    pub volume: u16,
    pub mute: bool,
}
```

#### Étape 4.3 — Enregistrer les routes dans `config.rs` (`browser/mod.rs`)

```rust
let dynamic_router = Router::new()
    // ... routes existantes ...
    .route("/{id}/nowplaying", get(nowplaying_handler))
    .route("/{id}/state", get(state_handler))
    .with_state(registry.clone());
```

---

### Phase 5 — Restructuration des modules (si phases 1–4 terminées)

La restructuration en `core/` vs `browser/` peut être faite en dernier, une fois que toutes les
interfaces sont stabilisées. C'est une refactorisation pure (déplacements + re-exports), sans
changement de logique.

**Ordre de déplacement** (évite les cycles de compilation) :
1. Créer `core/adapter.rs` avec `DeviceCommand` + `DeviceAdapter` trait
2. Déplacer `state.rs` → `core/state.rs`
3. Déplacer `messages.rs` → `core/messages.rs`
4. Déplacer `error.rs` → `core/error.rs`
5. Déplacer `pipeline.rs` → `core/pipeline.rs`
6. Déplacer `handlers.rs` → `core/handlers.rs`
7. Déplacer `renderer.rs` → `core/renderer.rs`
8. Déplacer `registry.rs` → `core/registry.rs`
9. Créer `browser/adapter.rs` avec `BrowserAdapter`
10. Déplacer `register.rs` → `browser/register.rs`
11. Déplacer `stream.rs` → `browser/stream.rs`
12. Mettre à jour `lib.rs` et `config.rs`

À chaque déplacement : `cargo check -p pmowebrenderer` avant de passer au suivant.

---

## Ordre d'exécution global

| Phase | Fichiers principaux | Dépendances |
|-------|---------------------|-------------|
| 0 | `core/handlers.rs` | aucune |
| 1.1–1.3 | `core/adapter.rs`, `core/state.rs` | aucune |
| 1.4 | `browser/adapter.rs` | 1.1–1.3 |
| 1.5–1.6 | `core/registry.rs`, `browser/register.rs` | 1.4 |
| 2.1 | `core/pipeline.rs` | 1.1 |
| 2.2–2.6 | `core/handlers.rs`, `core/renderer.rs` | 2.1, 1.4 |
| 3.1–3.2 | `PMOPlayer.ts` | aucune |
| 3.3 | `core/registry.rs` | aucune |
| 4.1–4.3 | `browser/register.rs`, `config.rs` | 1.5 |
| 5 | tous | 1–4 terminées |

Les phases 3 (TypeScript) et 0 (bug P0) peuvent être faites à tout moment en parallèle.

---

## Points d'attention pour l'implémentation

### `DeviceAdapter` dans les handlers UPnP

Les handlers UPnP sont créés à la construction du device (dans `WebRendererFactory`), avant que
l'instance browser soit enregistrée. L'adapter doit donc être créé **en même temps que le pipeline**,
dans `create_instance()`, et passé à la factory via `create_device_with_pipeline()`.

### `Weak<dyn DeviceAdapter>` dans `run_event_listener`

`WebRendererInstance` détient `adapter: Arc<dyn DeviceAdapter>`. Si `run_event_listener` détient
aussi un `Arc`, on a un cycle : `Instance → pipeline → event_listener → Instance`. Utiliser
`Weak` dans l'event listener et `.upgrade()` à l'usage.

### `createMediaElementSource()` et CORS

`AudioContext.createMediaElementSource()` requiert que la src audio soit de la même origine ou
serve les headers CORS appropriés. Le stream `/api/webrenderer/{id}/stream` est sur la même
origine que la webapp — pas de problème en pratique.

### Backward compatibility de l'endpoint `/command`

Le endpoint `GET /api/webrenderer/{id}/command` existant retourne actuellement un seul objet JSON
ou 204. Avec la queue, il peut retourner la prochaine commande (comportement identique côté
browser — `PMOPlayer.ts` appelle `/command` en boucle).

### `flac_handle.resume()` dans `play_handler`

Appeler `resume()` même si le flux n'était pas pausé est idempotent (atomic store). Pas de
vérification de l'état préalable nécessaire.

---

## Vérification finale

```bash
# Compilation sans erreur
cargo check -p pmowebrenderer

# Avec feature pmoserver
cargo check -p pmowebrenderer --features pmoserver

# Test fonctionnel browser :
# 1. BubbleUPnP → SetAVTransportURI → Play : stream démarre, pas de Transitioning bloqué
# 2. BubbleUPnP → Pause : silence envoyé au browser (<audio> pause quasi-immédiate)
# 3. BubbleUPnP → Play (reprise) : lecture reprend
# 4. BubbleUPnP → Stop : browser flush buffer, arrêt
# 5. BubbleUPnP → Next : flush + nouveau stream, transition <1s
# 6. Couper le réseau 5s puis le rétablir : auto-reconnect browser
# 7. GET /api/webrenderer/{id}/nowplaying : JSON valide avec HH:MM:SS
# 8. GET /api/webrenderer/{id}/state : JSON valide avec tous les champs
```

---

## Rapport d'exécution (2026-04-05)

### Ce qui a été réalisé

| Phase | Statut | Notes |
|-------|--------|-------|
| **Phase 0** — Bug P0 (`play_handler` sans URI) | ✅ Complet | Early return avant tout changement d'état |
| **Phase 1.1** — `DeviceCommand` enum | ✅ Complet | Dans `src/adapter.rs` (pas encore `core/`) |
| **Phase 1.2** — Trait `DeviceAdapter` | ✅ Complet | Dans `src/adapter.rs` |
| **Phase 1.3** — `VecDeque<DeviceCommand>` dans `RendererState` | ✅ Complet | `push_command`/`pop_command` ok |
| **Phase 1.4** — `BrowserAdapter` | ✅ Complet | Dans `src/adapter.rs` (pas encore `browser/`) |
| **Phase 1.5** — `adapter` dans `WebRendererInstance` | ❌ Non fait | Pas de champ `adapter` dans la struct |
| **Phase 1.6** — Nettoyage méthodes browser dans `RendererRegistry` | ❌ Non fait | `set_player_command`, `get_pending_command`, `has_current_uri`, `send_play_command`, `send_pause_command` toujours présents |
| **Phase 2.1** — `flac_handle` dans `PipelineHandle` | ✅ Complet | Exposé dans `PipelineHandle` |
| **Phase 2.2** — `pause_handler` appelle `flac_handle.pause()` | ✅ Complet | |
| **Phase 2.3** — `stop_handler` envoie `Flush + Stop` au device | ⚠️ Partiel | Appelle `flac_handle.pause()` mais n'envoie **pas** `Flush`/`Stop` via adapter (adapter non câblé) |
| **Phase 2.4** — `run_event_listener` avec `Weak<dyn DeviceAdapter>`, `Flush` sur `TrackEnded` | ❌ Non fait | Pas de `Weak<DeviceAdapter>`, pas de `Flush` envoyé au browser sur fin de piste |
| **Phase 2.5** — `play_handler` appelle `flac_handle.resume()` | ✅ Complet | |
| **Phase 2.6** — `build_avtransport` avec paramètre `adapter` | ❌ Non fait | Signature inchangée |
| **Phase 3.1** — `AudioContext` dans `PMOPlayer.ts` | ✅ Complet | `ensureAudioContext()`, `ac?.suspend()` dans `flush()` |
| **Phase 3.2** — Auto-reconnect avec backoff exponentiel | ✅ Complet | `scheduleReconnect()`, 5 tentatives max |
| **Phase 3.3** — Unification format position (`seconds_to_upnp_time`) | ✅ Complet | `update_player_state` corrigé |
| **Phase 4.1** — `GET /{id}/nowplaying` | ✅ Complet | Dans `register.rs` |
| **Phase 4.2** — `GET /{id}/state` | ✅ Complet | Dans `register.rs` |
| **Phase 4.3** — Routes enregistrées dans `config.rs` | ✅ Complet | |
| **Phase 5** — Restructuration `core/` vs `browser/` | ⏸️ Différé | Décision explicite |

---

## Tâches restantes

### T1 — Câbler `adapter` dans `WebRendererInstance` et handlers (Phase 1.5 + 2.6)

**Problème** : le `BrowserAdapter` est implémenté mais jamais instancié ni utilisé.
Les handlers `stop_handler` et `pause_handler` appellent `flac_handle.pause()` mais n'envoient
pas les commandes `Flush`/`Stop`/`Pause` au browser via l'adapter.

**Fichiers** : `registry.rs`, `renderer.rs`, `handlers.rs`

**Étapes** :

1. Dans `WebRendererInstance` (`registry.rs`), ajouter le champ :
   ```rust
   pub adapter: Arc<dyn crate::adapter::DeviceAdapter>,
   ```

2. Dans `create_instance()` (`registry.rs`), construire le `BrowserAdapter` avant la factory :
   ```rust
   let adapter: Arc<dyn crate::adapter::DeviceAdapter> =
       Arc::new(crate::adapter::BrowserAdapter { state: state.clone() });
   // Passer à la factory, stocker dans WebRendererInstance
   ```

3. Mettre à jour `WebRendererFactory::create_device_with_pipeline()` et `build_avtransport()`
   pour accepter `adapter: Arc<dyn DeviceAdapter>` et le passer aux handlers `pause_handler`,
   `stop_handler`, `play_handler`.

4. Dans `pause_handler` : ajouter `adapter.deliver(DeviceCommand::Pause)`.

5. Dans `stop_handler` : ajouter `adapter.deliver(DeviceCommand::Flush)` puis
   `adapter.deliver(DeviceCommand::Stop)`.

---

### T2 — `Flush` sur `TrackEnded` dans `run_event_listener` (Phase 2.4)

**Problème** : lors d'un changement de piste automatique, le browser a plusieurs secondes
d'audio bufférisé. Sans commande `Flush`, la transition de piste a un délai de 3–5 secondes.

**Fichiers** : `pipeline.rs`

**Étapes** :

1. Ajouter `adapter: std::sync::Weak<dyn crate::adapter::DeviceAdapter>` à la signature de
   `run_event_listener` et à l'appel dans `InstancePipeline::start()`.

2. Dans le bras `PlayerEvent::TrackEnded` :
   ```rust
   if let Some(adapter) = adapter.upgrade() {
       adapter.deliver(crate::adapter::DeviceCommand::Flush);
   }
   ```

3. Dans `InstancePipeline::start()`, passer `Arc::downgrade(&instance_adapter)` — nécessite
   que T1 soit terminé (adapter créé avant `start()`).

**Précaution** : utiliser `Weak` pour éviter le cycle de référence
`WebRendererInstance → pipeline → event_listener → WebRendererInstance`.

---

### T3 — Nettoyer `RendererRegistry` des méthodes browser-spécifiques (Phase 1.6)

**Problème** : `set_player_command`, `get_pending_command`, `has_current_uri`,
`send_play_command`, `send_pause_command` sont des fuites d'abstraction browser dans le registre
générique. Tout futur adaptateur (Android Auto…) devrait contourner ou dupliquer ces méthodes.

**Fichiers** : `registry.rs`, `register.rs`

**Condition préalable** : T1 terminé (l'adapter est accessible via `get_instance()`).

**Étapes** :

1. Ajouter `get_instance(&self, instance_id: &str) -> Option<Arc<WebRendererInstance>>`
   dans `RendererRegistry` (accès générique, remplace les méthodes spécialisées).

2. Déplacer dans `register.rs` la logique actuellement dans les méthodes à supprimer :
   - `get_pending_command` : `state.write().pop_command()` + sérialisation JSON → déjà fait dans `command_handler`
   - `set_player_command` : remplacé par `instance.adapter.deliver(cmd)`
   - `has_current_uri` : inline dans `play_handler` HTTP
   - `send_play_command` / `send_pause_command` : accès direct au pipeline via `get_instance`

3. Supprimer les 5 méthodes de `RendererRegistry`.

4. `cargo check -p pmowebrenderer` après chaque suppression.

---

### T4 — Phase 5 : Restructuration `core/` vs `browser/` (différé)

À faire une fois T1–T3 terminés et les interfaces stabilisées.
Voir la section "Phase 5" du plan ci-dessus pour l'ordre de déplacement.
Condition : `cargo check -p pmowebrenderer` doit passer à chaque étape.
