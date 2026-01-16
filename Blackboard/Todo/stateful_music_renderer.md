# Tâche : Rendre MusicRenderer complètement stateful

## Objectif

Refactoriser l'architecture pour que chaque `MusicRenderer` gère son propre état complet et son thread de surveillance, au lieu de déléguer le polling au `ControlPoint`.

## Motivation

1. **Encapsulation** - Tout l'état et le comportement d'un renderer au même endroit
2. **Cohérence** - Les événements sont émis là où l'état change
3. **Adaptabilité par backend** - Chaque backend peut avoir sa propre stratégie de surveillance (polling vs push pour OpenHome/Chromecast)
4. **Auto-advance spécifique** - La logique d'auto-advance peut être adaptée par backend
5. **Simplicité du ControlPoint** - Il devient un simple registry/coordinateur

## Contraintes

- Moins de 10 renderers simultanés → 10 threads de surveillance n'est pas un problème
- Le transfert de queue et autres opérations multi-renderers n'impliquent pas de surveillance

## Architecture cible

### MusicRenderer

Responsabilités :
- Maintenir l'état complet du renderer (transport, volume, position, queue, binding)
- Gérer son propre thread de surveillance (polling ou push selon le backend)
- Émettre tous les événements (StateChanged, PositionChanged, VolumeChanged, MuteChanged, QueueUpdated, BindingChanged)
- Gérer l'auto-advance de la queue (logique adaptée par backend)

Nouveau champ :
```rust
pub struct MusicRenderer {
    // ... champs existants ...
    event_bus: Option<RendererEventBus>,
    // Nouveau : état surveillé
    watched_state: Arc<Mutex<WatchedState>>,
    // Nouveau : handle du thread de surveillance
    watcher_handle: Option<JoinHandle<()>>,
}

struct WatchedState {
    last_playback_state: Option<PlaybackState>,
    last_position: Option<PlaybackPositionInfo>,
    last_volume: Option<u16>,
    last_mute: Option<bool>,
}
```

Nouvelles méthodes :
```rust
impl MusicRenderer {
    /// Démarre le thread de surveillance
    pub fn start_watching(&self) -> Result<(), ControlPointError>;
    
    /// Arrête le thread de surveillance
    pub fn stop_watching(&self);
    
    /// Logique d'auto-advance (appelée quand state passe à Stopped)
    fn handle_playback_stopped(&self);
}
```

### ControlPoint

Responsabilités simplifiées :
- Registry des devices (renderers et servers)
- Coordination des opérations multi-renderers (transfer_queue)
- Point d'entrée API pour les couches supérieures
- Démarrage/arrêt des watchers lors de l'ajout/suppression de renderers

Supprimer :
- Le polling loop centralisé (`start_polling_loop`)
- Les snapshots de surveillance (`RendererSnapshot`)
- La logique d'auto-advance centralisée

### MusicRendererBackend

Enrichir le trait pour supporter différentes stratégies de surveillance :
```rust
pub trait BackendWatcher {
    /// Retourne la stratégie de surveillance pour ce backend
    fn watch_strategy(&self) -> WatchStrategy;
}

pub enum WatchStrategy {
    /// Polling à intervalle fixe (UPnP, LinkPlay, Arylic)
    Polling { interval_ms: u64 },
    /// Notifications push (OpenHome, Chromecast)
    Push,
    /// Hybride : push avec polling de secours
    Hybrid { polling_interval_ms: u64 },
}
```

## Étapes d'implémentation

### Étape 1 : Préparer MusicRenderer

**Crate** : `pmocontrol`

1. Ajouter `WatchedState` et les champs associés à `MusicRenderer`
2. Implémenter `start_watching()` et `stop_watching()`
3. Implémenter la boucle de surveillance interne avec émission d'événements
4. Implémenter `handle_playback_stopped()` pour l'auto-advance

### Étape 2 : Adapter par backend

**Crate** : `pmocontrol`

1. Définir le trait `BackendWatcher` et `WatchStrategy`
2. Implémenter pour chaque backend :
   - `UpnpRenderer` : Polling 500ms
   - `OpenHomeRenderer` : Push (via subscriptions UPnP) avec fallback polling
   - `LinkPlayRenderer` : Polling 500ms
   - `ArylicTcpRenderer` : Polling 500ms
   - `ChromecastRenderer` : Push avec fallback polling
   - `HybridUpnpArylic` : Polling 500ms

### Étape 3 : Simplifier ControlPoint

**Crate** : `pmocontrol`

1. Supprimer `start_polling_loop()` et code associé
2. Supprimer `RendererSnapshot` et la gestion des snapshots
3. Modifier `push_renderer()` dans Registry pour appeler `start_watching()`
4. Modifier la gestion offline pour appeler `stop_watching()`
5. Supprimer la logique d'auto-advance du `handle_renderer_event()`

### Étape 4 : Tests et validation

1. Vérifier que les événements SSE sont toujours émis correctement
2. Vérifier l'auto-advance pour chaque type de backend
3. Vérifier le comportement online/offline
4. Tests de performance avec plusieurs renderers

## Fichiers impactés

| Fichier | Modification |
|---------|--------------|
| `pmocontrol/src/music_renderer/musicrenderer.rs` | Ajout état surveillé, thread watcher, auto-advance |
| `pmocontrol/src/music_renderer/mod.rs` | Ajout trait `BackendWatcher` |
| `pmocontrol/src/music_renderer/upnp_renderer.rs` | Impl `BackendWatcher` (Polling) |
| `pmocontrol/src/music_renderer/openhome_renderer.rs` | Impl `BackendWatcher` (Push/Hybrid) |
| `pmocontrol/src/music_renderer/linkplay_renderer.rs` | Impl `BackendWatcher` (Polling) |
| `pmocontrol/src/music_renderer/arylic_tcp.rs` | Impl `BackendWatcher` (Polling) |
| `pmocontrol/src/music_renderer/chromecast_renderer.rs` | Impl `BackendWatcher` (Push/Hybrid) |
| `pmocontrol/src/control_point.rs` | Suppression polling loop, simplification |
| `pmocontrol/src/registry.rs` | Appel start/stop watching |

## Notes

- Ce refactoring est significatif mais améliore la maintenabilité à long terme
- La migration peut être faite de manière incrémentale en gardant temporairement les deux systèmes
- Les backends OpenHome et Chromecast bénéficieront particulièrement de cette architecture (notifications push natives)
