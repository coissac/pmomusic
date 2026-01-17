# Rapport: Fix Backend Mutex Poisoned

## Résumé

Ce rapport documente la correction du bug "Backend mutex poisoned" qui se manifestait lors de l'arrêt de la lecture sur un renderer OpenHome, ainsi que la régression Round 2 découverte après les premiers correctifs.

## Analyse de la cause racine

### Round 1 : Mutex empoisonné

Le mutex backend était empoisonné par des panics non gérés lors d'opérations sur les renderers. Les appels `.unwrap()` et `.expect()` sur le mutex propageaient les panics au lieu de les gérer gracieusement.

### Round 2 : Régression après Round 1

Les correctifs du Round 1 ont révélé un problème plus profond. La chaîne d'échecs était :

1. **DeleteAll échoue avec erreur 501** : Le renderer OpenHome rejette l'action `DeleteAll` pendant la lecture active
2. **Le code continue** (grâce aux correctifs Round 1 qui tolèrent les erreurs)
3. **État incohérent du renderer** : OpenHome retourne 5 IDs via `IdArray` mais une `<TrackList>` vide via `ReadList`
4. **Panic "index out of bounds"** : `sync_queue()` accède à `items[4]` alors que `items.len() == 0`
5. **Mutex empoisonné** : Le panic dans le thread empoisonne le mutex

Preuve dans les logs :
```
OpenHome Playlist IdArray returned ... id_count=5
OpenHome Playlist tracks read ... track_count=0 expected_count=5
```

## Corrections apportées

### 1. Tolérance des erreurs clear_queue (`musicrenderer.rs`)

**Fichier** : `pmocontrol/src/music_renderer/musicrenderer.rs`

**Modification** : La méthode `clear_for_playlist_attach()` tolère maintenant les erreurs de `clear_queue()` au lieu de propager l'erreur.

```rust
pub fn clear_for_playlist_attach(&self) -> Result<(), ControlPointError> {
    let mut backend = self.lock_backend_for("clear_for_playlist_attach");

    // Clear the queue first (ignore errors - queue will be replaced anyway by sync_queue)
    // Some backends (OpenHome) may reject DeleteAll if currently playing
    if let Err(err) = backend.clear_queue() {
        warn!(
            renderer = self.id().0.as_str(),
            error = %err,
            "Clear queue failed when preparing for playlist attach (continuing anyway)"
        );
    }

    // Then stop playback (ignore errors if already stopped)
    backend.stop().or_else(|err| {
        warn!(
            renderer = self.id().0.as_str(),
            error = %err,
            "Stop failed when preparing for playlist attach (continuing anyway)"
        );
        Ok(())
    })
}
```

**Justification** : Le `DeleteAll` n'est pas critique car `sync_queue()` remplacera de toute façon le contenu de la queue.

### 2. Suppression du clear_queue redondant (`control_point.rs`)

**Fichier** : `pmocontrol/src/control_point.rs`

**Modification** : Suppression de l'appel `renderer.clear_queue()?` dans `attach_queue_to_playlist_internal()`.

Avant :
```rust
// Clear the local queue (detach binding + clear runtime queue structure)
self.detach_playlist_binding(renderer_id, "attach_new_playlist");
renderer.clear_queue()?;
```

Après :
```rust
// Detach any existing binding (local queue will be replaced by sync_queue later)
self.detach_playlist_binding(renderer_id, "attach_new_playlist");
```

**Justification** : Ce `clear_queue()` était redondant car `clear_for_playlist_attach()` le fait déjà, et causait un second échec `DeleteAll`.

### 3. Bounds-check pour current_index (`openhome.rs`)

**Fichier** : `pmocontrol/src/queue/openhome.rs`

**Modification** : Ajout d'une vérification de bornes dans `sync_queue()` pour gérer l'état incohérent du renderer OpenHome.

```rust
let snapshot = self.queue_snapshot()?;
// Note: current_index may point to an index that doesn't exist in items
// if the OpenHome renderer is in an inconsistent state (e.g., IdArray returns
// IDs but ReadList returns empty TrackList). We must bounds-check here.
let playing_info = snapshot.current_index.and_then(|idx| {
    if idx < snapshot.items.len() {
        Some((
            idx,
            snapshot.items[idx].backend_id,
            snapshot.items[idx].uri.clone(),
            snapshot.items[idx].didl_id.clone(),
        ))
    } else {
        warn!(
            renderer = self.renderer_id.0.as_str(),
            current_index = idx,
            items_len = snapshot.items.len(),
            "OpenHome renderer in inconsistent state: current_index out of bounds, treating as no current track"
        );
        None
    }
});
```

**Justification** : Gère le cas où le renderer OpenHome retourne un état incohérent (IDs sans données de track correspondantes).

### 4. Ajout de l'import warn (`openhome.rs`)

**Fichier** : `pmocontrol/src/queue/openhome.rs`

**Modification** : Ajout de `warn` à l'import tracing.

```rust
use tracing::{debug, warn};
```

## Fichiers modifiés

| Fichier | Modification |
|---------|-------------|
| `pmocontrol/src/music_renderer/musicrenderer.rs` | Tolérance des erreurs `clear_queue()` dans `clear_for_playlist_attach()` |
| `pmocontrol/src/control_point.rs` | Suppression du `clear_queue()` redondant |
| `pmocontrol/src/queue/openhome.rs` | Bounds-check + import `warn` |

## Comportement attendu après correction

1. **DeleteAll échoue** : Warning loggé, le code continue
2. **État incohérent détecté** : Warning loggé, traité comme "pas de track courante"
3. **sync_queue réussit** : La playlist est correctement attachée au renderer
4. **Pas de panic** : Le mutex reste sain

## Tests effectués

- L'utilisateur a confirmé que la correction fonctionne ("Ok ça marche")

## Notes techniques

Les correctifs du Round 1 (gestion d'erreur sur mutex) ont révélé un bug préexistant : le code supposait que l'état du renderer OpenHome était toujours cohérent. En réalité, certains renderers peuvent retourner des IDs de tracks sans les données correspondantes, notamment lorsqu'une opération `DeleteAll` est rejetée pendant la lecture.

La solution adoptée est défensive : plutôt que de supposer un état cohérent, le code vérifie les bornes et traite les incohérences comme des cas dégradés (pas de track courante) plutôt que de paniquer.
