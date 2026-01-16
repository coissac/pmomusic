# Rapport : Rendre MusicRenderer complètement stateful

## Résumé

Refactorisation de l'architecture pour que chaque `MusicRenderer` gère son propre thread de surveillance (watcher), au lieu de déléguer le polling au `ControlPoint` centralisé. Cette modification améliore l'encapsulation, la cohérence des événements et prépare le terrain pour le support futur des notifications push (OpenHome, Chromecast).

## Travail effectué

### Phase 1 : Création du module watcher.rs

**Fichier créé** : `pmocontrol/src/music_renderer/watcher.rs`

Nouveau module contenant :
- `WatchStrategy` enum avec trois variantes :
  - `Polling { interval_ms: u64 }` - pour UPnP, LinkPlay, Arylic (500ms)
  - `Push` - pour support futur des notifications push
  - `Hybrid { polling_interval_ms: u64 }` - pour OpenHome et Chromecast
- `WatchedState` struct pour le cache de détection des changements
- Fonctions helper déplacées depuis `control_point.rs` :
  - `playback_state_equal()`
  - `playback_position_equal()`
  - `compute_logical_playback_state()`
  - `extract_track_metadata()`
  - `parse_hms_to_secs()`
- Tests unitaires pour les fonctions helper

### Phase 2 : Extension de MusicRenderer

**Fichier modifié** : `pmocontrol/src/music_renderer/musicrenderer.rs`

Nouveaux champs ajoutés à la struct `MusicRenderer` :
- `watched_state: Arc<Mutex<WatchedState>>` - cache pour détection des changements
- `watcher_stop_flag: Arc<AtomicBool>` - signal d'arrêt du thread
- `watcher_handle: Arc<Mutex<Option<JoinHandle<()>>>>` - handle du thread watcher

Nouvelles méthodes publiques :
- `start_watching()` - démarre le thread de surveillance (idempotent)
- `stop_watching()` - arrête le thread gracieusement (idempotent)
- `is_watching()` - retourne l'état du watcher

Nouvelles méthodes internes :
- `spawn_watcher_thread()` - crée le thread avec la stratégie appropriée
- `watcher_loop()` - boucle principale de polling
- `poll_and_emit_changes()` - poll le backend et émet les événements
- `handle_state_change()` - logique d'auto-advance (déplacée depuis ControlPoint)
- `emit_event()` - helper pour émettre un événement via le bus

### Phase 3 : Modification du Registry

**Fichier modifié** : `pmocontrol/src/registry.rs`

Ajout des appels `start_watching()` / `stop_watching()` :
- `push_renderer()` : appelle `start_watching()` quand un renderer arrive en ligne ou est créé
- `device_says_byebye()` : appelle `stop_watching()` avant de marquer offline
- `check_timeouts()` : appelle `stop_watching()` avant de marquer offline sur timeout

### Phase 4 : Simplification du ControlPoint

**Fichier modifié** : `pmocontrol/src/control_point.rs`

Suppressions :
- Thread de polling central (~140 lignes)
- Struct `RendererRuntimeSnapshot`
- Méthodes `emit_renderer_event()` et `handle_renderer_event()`
- Fonctions helper déplacées vers `watcher.rs`

### Phase 5 : Mise à jour du module

**Fichier modifié** : `pmocontrol/src/music_renderer/mod.rs`

Ajout de `pub mod watcher;` pour exposer le nouveau module.

## Liste des fichiers

### Fichiers créés

| Fichier | Description |
|---------|-------------|
| `pmocontrol/src/music_renderer/watcher.rs` | Module watcher avec WatchStrategy, WatchedState et fonctions helper |

### Fichiers modifiés

| Fichier | Modification |
|---------|--------------|
| `pmocontrol/src/music_renderer/musicrenderer.rs` | Ajout champs watcher, méthodes start/stop_watching, logique auto-advance |
| `pmocontrol/src/music_renderer/mod.rs` | Ajout `pub mod watcher;` |
| `pmocontrol/src/registry.rs` | Appels start/stop_watching dans push_renderer, device_says_byebye, check_timeouts |
| `pmocontrol/src/control_point.rs` | Suppression polling central, RendererRuntimeSnapshot, handle_renderer_event, fonctions helper |

## Notes techniques

- Le signal d'arrêt utilise `AtomicBool` avec `Ordering::SeqCst` pour garantir la visibilité entre threads
- Les méthodes `start_watching()` et `stop_watching()` sont idempotentes
- Le thread watcher est nommé `watcher-{friendly_name}` pour faciliter le debug
- L'intervalle de polling est de 500ms (volume/mute toutes les 2 ticks = 1s)
- La logique `compute_logical_playback_state()` compense les bugs des devices Arylic/LinkPlay
- L'auto-advance est maintenant géré directement dans le watcher du MusicRenderer

## Round 2 : Vérification transition offline → online

### Problème identifié

La méthode `refresh_device_presence()` dans `registry.rs` n'appelait pas `start_watching()` quand un renderer passait de offline à online. Cette méthode est appelée lors de la réception de messages SSDP Alive.

### Correction appliquée

**Fichier modifié** : `pmocontrol/src/registry.rs`

Ajout de l'appel `renderer.start_watching()` dans `refresh_device_presence()` quand `was_online == false`.

### Points de démarrage du watcher vérifiés

| Méthode | Situation | `start_watching()` appelé |
|---------|-----------|---------------------------|
| `push_renderer()` | Nouveau renderer | Oui |
| `push_renderer()` | Renderer existant, était offline | Oui |
| `refresh_device_presence()` | Renderer existant, était offline | Oui (corrigé) |

### Points d'arrêt du watcher vérifiés

| Méthode | Situation | `stop_watching()` appelé |
|---------|-----------|--------------------------|
| `device_says_byebye()` | SSDP ByeBye reçu | Oui |
| `check_timeouts()` | Timeout dépassé | Oui |

## Round 3 : Audit complet de la logique offline/online

Suite à la découverte du manque dans le Round 2, un audit complet de tous les chemins offline/online a été effectué.

### Chemins qui appellent `start_watching()`

| Chemin | Fonction | Ligne | Condition | Status |
|--------|----------|-------|-----------|--------|
| Nouveau renderer découvert | `push_renderer()` | 180, 194 | Création nouvelle entry | ✅ OK |
| Renderer existant, ajout renderer à entry | `push_renderer()` | 169 | Entry existe sans renderer | ✅ OK |
| Renderer existant revient online | `push_renderer()` | 160 | `!was_online` | ✅ OK |
| SSDP Alive pour device connu | `refresh_device_presence()` | 269 | `!was_online` | ✅ OK (corrigé Round 2) |

### Chemins qui appellent `stop_watching()`

| Chemin | Fonction | Ligne | Condition | Status |
|--------|----------|-------|-----------|--------|
| SSDP ByeBye reçu | `device_says_byebye()` | 289 | Renderer présent | ✅ OK |
| Timeout dépassé | `check_timeouts()` | 308 | `elapsed > max_age` | ✅ OK |

### Analyse des flux

```
┌─────────────────────────────────────────────────────────────────┐
│                    FLUX ONLINE                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  SSDP Discovery ──► push_renderer() ──► start_watching() ✅     │
│                                                                  │
│  SSDP Alive (nouveau UDN) ──► push_renderer() ──► start_watching() ✅ │
│                                                                  │
│  SSDP Alive (UDN connu, online) ──► refresh_device_presence()   │
│                                     (pas de start car déjà en marche) │
│                                                                  │
│  SSDP Alive (UDN connu, offline) ──► refresh_device_presence()  │
│                                       ──► start_watching() ✅    │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                    FLUX OFFLINE                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  SSDP ByeBye ──► device_says_byebye() ──► stop_watching() ✅    │
│                                                                  │
│  Timeout ──► check_timeouts() ──► stop_watching() ✅            │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Conclusion

**Tous les chemins sont correctement gérés.** Chaque transition offline→online appelle `start_watching()` et chaque transition online→offline appelle `stop_watching()`.

L'idempotence des méthodes `start_watching()` et `stop_watching()` garantit qu'aucun problème ne survient en cas d'appels multiples.

## Round 4 : Centralisation de la gestion du watcher

### Problème identifié

Les appels à `start_watching()` et `stop_watching()` étaient dispersés dans `registry.rs` (6 emplacements), augmentant le risque d'oubli (comme découvert en Round 2).

### Solution implémentée

Centralisation de la gestion du watcher dans `MusicRenderer` lui-même :

1. **Constructeur** (`from_renderer_info_with_bus()`) : appelle automatiquement `start_watching()` à la fin, car le renderer est créé avec `online = true`

2. **`has_been_seen_now()`** : appelle automatiquement `start_watching()` si transition offline→online

3. **`mark_as_offline()`** : appelle automatiquement `stop_watching()` avant de passer offline

### Fichiers modifiés

| Fichier | Modification |
|---------|--------------|
| `pmocontrol/src/music_renderer/musicrenderer.rs` | Ajout `start_watching()` dans constructeur, dans `has_been_seen_now()` et `stop_watching()` dans `mark_as_offline()` |
| `pmocontrol/src/registry.rs` | Suppression de tous les appels manuels à `start_watching()` et `stop_watching()` |

### Avantages

- **Encapsulation** : la logique watcher est entièrement gérée par `MusicRenderer`
- **Impossible d'oublier** : les transitions sont automatiquement gérées
- **Code simplifié** : `registry.rs` ne contient plus de logique watcher
- **Idempotence** : les appels multiples sont sans effet grâce aux guards existants

### Nouvelle architecture

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

## Compilation

Le projet compile sans erreur.
