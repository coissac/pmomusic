# Bug : Duplication de piste en position 0 lors de la lecture

**Statut** : Terminé  
**Crate** : pmocontrol

---

## Description initiale du bug

### Contexte
Lecture d'une playlist liée (bindée) à une queue OpenHome ou interne.

### Comportement observé
1. Sélection d'une playlist → la queue se charge correctement
2. Lecture démarre à la piste 1 → OK
3. Fin de la piste 1 → passage à la piste 2
4. **BUG** : Après un certain délai (~60s), la piste 2 est **dupliquée en position 0**
5. La lecture continue depuis cette nouvelle position 0

### Symptôme clé
Toute piste en cours de lecture finit par être dupliquée en première position de la queue.

---

## Analyse et cause racine

### Mécanisme du bug
La fonction `sync_queue` (appelée lors des refreshes périodiques de playlist toutes les 60 secondes) comparait les items **uniquement par leur URI**. 

Si le MediaServer retournait une URI légèrement différente pour le même morceau (tokens de session, encodage différent, etc.), l'item courant n'était pas reconnu dans la nouvelle playlist et était préservé en position 0, créant une duplication.

### Flux problématique
```
1. Playlist attachée → lecture piste N
2. Refresh périodique (60s) → sync_queue()
3. Comparaison URI courante vs URIs playlist
4. URI non trouvée → piste préservée en position 0
5. Résultat : duplication
```

---

## Solution implémentée

### Principe
Extension de la logique de comparaison pour utiliser l'URI **OU** le `didl_id` comme critère d'identification. Le `didl_id` est l'identifiant DIDL-Lite stable assigné par le MediaServer, indépendant de l'URI de streaming.

### Fichiers modifiés

| Fichier | Modifications |
|---------|---------------|
| `pmocontrol/src/queue/interne.rs` | Comparaison par `didl_id` en fallback + logs diagnostic |
| `pmocontrol/src/queue/openhome.rs` | Fonction `items_match` + modification `sync_queue` et `lcs_flags` |

### Code clé

```rust
// pmocontrol/src/queue/openhome.rs
fn items_match(a: &PlaybackItem, b: &PlaybackItem) -> bool {
    a.uri == b.uri || a.didl_id == b.didl_id
}
```

```rust
// pmocontrol/src/queue/interne.rs
let new_idx = items.iter().position(|item| item.uri == current_uri)
    .or_else(|| items.iter().position(|item| item.didl_id == current_didl_id));
```

---

## Diagnostic

Pour activer les logs :

```bash
RUST_LOG=pmocontrol::queue=debug
```

Messages de trace :
- `sync_queue: current item found in new playlist` → Comportement normal
- `sync_queue: current item NOT found in new playlist` → Cas problématique (ne devrait plus apparaître)
