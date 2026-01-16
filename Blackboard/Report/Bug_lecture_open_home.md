# Rapport : Bug de duplication de piste en position 0

## Résumé

Correction d'un bug où la piste en cours de lecture était dupliquée en position 0 de la queue après un certain temps.

## Problème identifié

### Symptôme
Lors de la lecture d'une playlist liée à une queue (OpenHome ou interne), après le passage à une nouvelle piste, celle-ci finissait par être dupliquée en première position de la queue.

### Cause racine
La fonction `sync_queue` (utilisée lors des refreshes périodiques de playlist toutes les 60 secondes) comparait les items uniquement par leur URI. Si le MediaServer retournait une URI légèrement différente pour le même morceau (tokens de session, encodage différent, etc.), l'item courant n'était pas reconnu dans la nouvelle playlist et était préservé en position 0, créant ainsi une duplication.

### Mécanisme détaillé
1. Une playlist est attachée à un renderer
2. La lecture commence sur la piste N
3. Après 60 secondes, un refresh périodique déclenche `sync_queue`
4. `sync_queue` compare l'URI de la piste courante avec les URIs de la playlist rafraîchie
5. Si les URIs ne correspondent pas exactement, la piste courante est considérée comme "absente" de la playlist
6. La logique de préservation insère alors la piste courante en position 0
7. Résultat : duplication de la piste

## Solution appliquée

### Modification de la logique de comparaison

La comparaison des items a été étendue pour utiliser l'URI **OU** le `didl_id` comme critère d'identification. Le `didl_id` est l'identifiant DIDL-Lite stable assigné par le MediaServer, indépendant de l'URI de streaming.

### Fichiers modifiés

#### 1. `pmocontrol/src/queue/interne.rs`

- Ajout de la comparaison par `didl_id` en fallback dans `sync_queue`
- Ajout de logs de diagnostic pour tracer les cas de non-correspondance

```rust
// Avant
let new_idx = items.iter().position(|item| item.uri == current_uri);

// Après
let new_idx = items.iter().position(|item| item.uri == current_uri)
    .or_else(|| items.iter().position(|item| item.didl_id == current_didl_id));
```

#### 2. `pmocontrol/src/queue/openhome.rs`

- Ajout de la fonction `items_match` pour encapsuler la logique de comparaison
- Modification de `sync_queue` pour utiliser URI ou `didl_id`
- Modification de `lcs_flags` (algorithme LCS) pour utiliser la même logique de comparaison

```rust
fn items_match(a: &PlaybackItem, b: &PlaybackItem) -> bool {
    a.uri == b.uri || a.didl_id == b.didl_id
}
```

## Tests recommandés

1. Attacher une playlist à un renderer OpenHome
2. Lancer la lecture
3. Attendre plusieurs cycles de refresh (> 60 secondes)
4. Vérifier que la queue ne contient pas de duplications
5. Cliquer sur différentes pistes et vérifier le même comportement

## Diagnostic

Pour activer les logs de diagnostic :

```bash
RUST_LOG=pmocontrol::queue=debug
```

Les messages suivants permettent de tracer le comportement :
- `sync_queue: current item found in new playlist` - Comportement normal
- `sync_queue: current item NOT found in new playlist, preserving as first item` - Cas problématique (ne devrait plus apparaître avec le fix)
