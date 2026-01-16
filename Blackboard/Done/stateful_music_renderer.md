# Tâche terminée : Rendre MusicRenderer complètement stateful

## Objectif

Refactoriser l'architecture pour que chaque `MusicRenderer` gère son propre thread de surveillance (watcher), au lieu de déléguer le polling au `ControlPoint` centralisé.

## Motivation

1. **Encapsulation** - Tout l'état et le comportement d'un renderer au même endroit
2. **Cohérence** - Les événements sont émis là où l'état change
3. **Adaptabilité par backend** - Chaque backend peut avoir sa propre stratégie de surveillance (polling vs push)
4. **Auto-advance spécifique** - La logique d'auto-advance peut être adaptée par backend
5. **Simplicité du ControlPoint** - Il devient un simple registry/coordinateur

---

## Résumé de l'implémentation

### Fichiers créés

| Fichier | Description |
|---------|-------------|
| `pmocontrol/src/music_renderer/watcher.rs` | Module watcher avec `WatchStrategy`, `WatchedState` et fonctions helper |

### Fichiers modifiés

| Fichier | Modification |
|---------|--------------|
| `pmocontrol/src/music_renderer/musicrenderer.rs` | Champs watcher, méthodes `start/stop_watching()`, logique auto-advance, gestion automatique dans constructeur et `DeviceOnline` |
| `pmocontrol/src/music_renderer/mod.rs` | Export du module `watcher` |
| `pmocontrol/src/registry.rs` | Simplifié : plus d'appels manuels watcher |
| `pmocontrol/src/control_point.rs` | Suppression polling central (~140 lignes), `RendererRuntimeSnapshot`, `handle_renderer_event()` |

---

## Architecture finale

### WatchStrategy

```rust
pub enum WatchStrategy {
    Polling { interval_ms: u64 },      // UPnP, LinkPlay, Arylic (500ms)
    Push,                               // Futur : notifications push
    Hybrid { polling_interval_ms: u64 }, // OpenHome, Chromecast
}
```

### Gestion automatique du watcher

```
┌─────────────────────────────────────────────────────────────────┐
│                    GESTION AUTOMATIQUE DU WATCHER               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Création MusicRenderer ──► constructeur ──► start_watching()   │
│                                                                  │
│  has_been_seen_now() ──► si !was_online ──► start_watching()    │
│                                                                  │
│  mark_as_offline() ──► stop_watching() ──► online = false       │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Flux offline/online

```
┌─────────────────────────────────────────────────────────────────┐
│                         FLUX ONLINE                              │
├─────────────────────────────────────────────────────────────────┤
│  SSDP Discovery ──► push_renderer() ──► constructeur            │
│                                          ──► start_watching()   │
│                                                                  │
│  SSDP Alive (offline→online) ──► has_been_seen_now()            │
│                                   ──► start_watching()          │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                         FLUX OFFLINE                             │
├─────────────────────────────────────────────────────────────────┤
│  SSDP ByeBye / Timeout ──► mark_as_offline()                    │
│                             ──► stop_watching()                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Points techniques clés

- **Thread safety** : `AtomicBool` avec `Ordering::SeqCst` pour le signal d'arrêt
- **Idempotence** : `start_watching()` et `stop_watching()` sont idempotents
- **Nommage** : Thread nommé `watcher-{friendly_name}` pour debug
- **Polling** : 500ms pour position/état, 1s pour volume/mute
- **Auto-advance** : Géré dans `handle_state_change()` du MusicRenderer
- **Compensation bugs** : `compute_logical_playback_state()` corrige les comportements Arylic/LinkPlay

---

## Rounds de vérification

| Round | Objectif | Résultat |
|-------|----------|----------|
| 1 | Implémentation initiale | OK |
| 2 | Vérifier transition offline→online | Bug trouvé et corrigé dans `refresh_device_presence()` |
| 3 | Audit complet des chemins offline/online | Tous les chemins vérifiés OK |
| 4 | Centralisation dans `MusicRenderer` | Gestion automatique dans constructeur et `DeviceOnline` |

---

## Conclusion

L'architecture est maintenant plus robuste :
- Impossible d'oublier de démarrer/arrêter le watcher
- Le `registry.rs` est simplifié
- Préparation pour le support futur des notifications push (OpenHome, Chromecast)
