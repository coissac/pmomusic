# Rapport Final : Implémentation du Shuffle de la Queue de Lecture

## Résumé

Implémentation complète de la fonctionnalité de shuffle (mélange aléatoire) de la queue de lecture pour les Music Renderers dans PMOMusic. Cette fonctionnalité permet de randomiser l'ordre des morceaux dans la queue et de redémarrer la lecture depuis le premier morceau.

Au-delà de la fonctionnalité shuffle, ce travail a permis d'améliorer l'architecture en centralisant l'émission des événements SSE dans le `MusicRenderer` plutôt que dans le `ControlPoint`.

## Travail effectué

### Étape 1 : Implémentation de la méthode shuffle dans MusicRenderer

**Fichier modifié** : `pmocontrol/src/music_renderer/musicrenderer.rs`

Ajout de la méthode `shuffle_queue()` qui implémente la stratégie suivante :
1. Détache la queue de lecture d'une playlist si celle-ci est attachée
2. Arrête la lecture en cours
3. Prend un snapshot de la queue actuelle
4. Randomise l'ordre des morceaux avec `rand::seq::SliceRandom`
5. Remplace la queue avec les items mélangés
6. Redémarre la lecture au premier morceau

**Dépendances ajoutées** :
- `rand = "0.9"` dans `Cargo.toml` (workspace)
- `rand = { workspace = true }` dans `pmocontrol/Cargo.toml`

### Étape 2 : API REST et documentation OpenAPI

**Fichiers modifiés** :
- `pmocontrol/src/pmoserver_ext.rs` : Ajout du handler `shuffle_queue`
- `pmocontrol/src/openapi.rs` : Ajout du path dans la documentation OpenAPI

**Endpoint créé** :
```
POST /api/control/renderers/{renderer_id}/queue/shuffle
```

**Réponses** :
- `200` : Queue mélangée et lecture démarrée
- `400` : Queue vide
- `404` : Renderer non trouvé
- `504` : Timeout de la commande
- `500` : Erreur interne

### Étape 3 : Interface Vue.js

**Fichiers créés** :
- `pmoapp/webapp/src/components/pmocontrol/ShuffleControl.vue` : Nouveau composant bouton shuffle

**Fichiers modifiés** :
- `pmoapp/webapp/src/services/pmocontrol/api.ts` : Ajout de la méthode `shuffleQueue()`
- `pmoapp/webapp/src/components/unified/BottomTabBar.vue` : Intégration du bouton shuffle à côté du timer

**Design** :
- Bouton circulaire avec icône Shuffle (lucide-vue-next)
- Style cohérent avec le bouton Timer existant
- Animation de chargement pendant l'exécution
- Responsive (taille réduite sur mobile)

### Étape 4 : Émission automatique des événements SSE (Round 3)

**Problème identifié** : L'interface utilisateur ne se mettait pas à jour après un shuffle car aucun événement `QueueUpdated` n'était émis.

**Solution implémentée** : Le `MusicRenderer` stocke maintenant une référence optionnelle au `RendererEventBus` et émet automatiquement un événement `QueueUpdated` après chaque modification de la queue.

**Fichiers modifiés** :

| Fichier | Modification |
|---------|--------------|
| `pmocontrol/src/music_renderer/musicrenderer.rs` | Ajout du champ `event_bus: Option<RendererEventBus>`, constructeur `from_renderer_info_with_bus()`, méthode helper `emit_queue_updated()`, implémentation manuelle de `Debug` |
| `pmocontrol/src/registry.rs` | Passage du `RendererEventBus` lors de la création des renderers via `from_renderer_info_with_bus()` |

**Méthodes qui émettent désormais `QueueUpdated`** :
- `enqueue_items()` - Ajout d'items à la queue
- `sync_queue()` - Synchronisation de la queue
- `clear_queue()` - Vidage de la queue
- `replace_queue()` - Remplacement complet de la queue (utilisé par `shuffle_queue()`)
- `play_next_from_queue()` - Passage au morceau suivant
- `play_from_index()` - Lecture à un index spécifique

### Étape 5 : Refactoring des émissions d'événements (Round 4)

**Objectif** : Centraliser les émissions d'événements dans le `MusicRenderer` et supprimer les émissions redondantes du `ControlPoint`.

**Principe** : Puisque le `MusicRenderer` a maintenant accès au `RendererEventBus`, il est plus cohérent et maintenable que les événements soient émis au niveau du renderer plutôt que dispersés dans le `ControlPoint`.

#### Événements `QueueUpdated`

**Modifications dans `ControlPoint`** - Suppression des émissions redondantes dans :
- `clear_queue()`
- `enqueue_items_with_mode()`
- `shuffle_queue()`
- `play_next_from_queue()`

#### Événements `BindingChanged`

**Modifications dans `MusicRenderer`** :
- `set_playlist_binding()` : Émet `BindingChanged` uniquement si le binding change réellement
- `clear_playlist_binding()` : Émet `BindingChanged` uniquement s'il y avait un binding à supprimer
- Ajout de la méthode helper `emit_binding_changed()`

**Modifications dans `ControlPoint`** :
- `attach_queue_to_playlist_internal()` : Suppression de l'émission manuelle de `BindingChanged`
- `detach_playlist_binding()` : Suppression de l'émission manuelle, utilisation de `clear_playlist_binding()` au lieu de `set_playlist_binding(None)`

## Liste complète des fichiers modifiés

| Fichier | Type de modification |
|---------|---------------------|
| `Cargo.toml` (workspace) | Ajout dépendance `rand` |
| `pmocontrol/Cargo.toml` | Ajout dépendance `rand` |
| `pmocontrol/src/music_renderer/musicrenderer.rs` | Ajout `shuffle_queue()`, `event_bus`, émission d'événements automatique |
| `pmocontrol/src/control_point.rs` | Suppression des émissions d'événements redondantes |
| `pmocontrol/src/registry.rs` | Passage du `RendererEventBus` lors de la création des renderers |
| `pmocontrol/src/pmoserver_ext.rs` | Ajout handler REST `shuffle_queue` |
| `pmocontrol/src/openapi.rs` | Ajout documentation OpenAPI |
| `pmoapp/webapp/src/services/pmocontrol/api.ts` | Ajout méthode API `shuffleQueue()` |
| `pmoapp/webapp/src/components/unified/BottomTabBar.vue` | Intégration du bouton shuffle |

## Fichiers créés

| Fichier | Description |
|---------|-------------|
| `pmoapp/webapp/src/components/pmocontrol/ShuffleControl.vue` | Composant Vue.js du bouton shuffle |

## Notes techniques

- La méthode `shuffle_queue` détache automatiquement la playlist liée pour éviter que la queue soit écrasée par une mise à jour de la playlist
- Le shuffle utilise `rand::thread_rng()` pour une génération aléatoire de qualité
- L'endpoint REST utilise le même pattern async que les autres commandes de transport (spawn_blocking + timeout)
- Le timeout utilisé est `QUEUE_COMMAND_TIMEOUT` (10 secondes)
- L'émission des événements SSE est automatique via le `RendererEventBus` intégré au `MusicRenderer`
- L'implémentation manuelle de `Debug` pour `MusicRenderer` est nécessaire car `RendererEventBus` n'implémente pas `Debug`
- Les événements ne sont émis que lorsqu'il y a un changement effectif (pas d'événement `BindingChanged` si le binding était déjà `None`)

## Améliorations architecturales

Ce travail a posé les bases d'une meilleure architecture où le `MusicRenderer` est responsable de l'émission de ses propres événements. Une tâche de suivi a été créée (`Blackboard/Todo/stateful_music_renderer.md`) pour aller plus loin et rendre le `MusicRenderer` complètement stateful avec son propre thread de surveillance.
