# Bug transfert de queue entre renderers - RESOLU

## Tâche originale

**Crate concernée** : pmocontrol

**Problème rapporté** : Le transfert de la queue de lecture d'un renderer vers un autre ne semblait plus fonctionnel, avec des comportements erratiques. Le problème était potentiellement lié aux derniers changements sur le passage du renderer à un état stateless. L'interface utilisateur mettait également un certain temps à réagir.

---

## Synthèse de la résolution

### Cause racine

Dans la fonction `transfer_queue()` (`control_point.rs:1192-1291`), lorsqu'un binding de playlist existait sur le renderer source, le code appelait `attach_queue_to_playlist()` sur la destination **après** avoir rempli la queue avec les items sources.

Le problème : `attach_queue_to_playlist_internal()` effectue :
1. `clear_for_playlist_attach()` - efface la queue du renderer
2. `clear_queue()` - efface la queue locale  
3. `refresh_attached_queue_for()` - browse le serveur et **remplace** la queue

Cela **écrasait complètement** les items transférés avec `replace_queue()`, perdant le `current_index` et la position de lecture.

### Séquence problématique (avant correction)

```
1. source_snapshot = get_renderer_queue_snapshot(source)  // items + current_index
2. clear_renderer_queue(dest)
3. dest.replace_queue(source_snapshot.items, current_index)  // Queue remplie OK
4. attach_queue_to_playlist(dest, server, container)         // ÉCRASE TOUT
   └─> clear_for_playlist_attach()
   └─> clear_queue()
   └─> refresh_attached_queue_for() → browse serveur → replace queue
5. play() sur destination avec mauvaise queue
```

### Solution

Remplacement de l'appel `attach_queue_to_playlist()` par un transfert direct du binding sans déclencher de refresh :

```rust
// AVANT (problématique)
if let Some((server_id, container_id, _)) = source_binding {
    self.attach_queue_to_playlist(dest_renderer_id, server_id, container_id)?;
}

// APRÈS (corrigé)
if let Some((server_id, container_id, has_seen_update)) = source_binding.clone() {
    let binding = PlaylistBinding {
        server_id,
        container_id,
        has_seen_update,
        pending_refresh: false,      // Pas de refresh immédiat
        auto_play_on_refresh: false,
    };
    dest_renderer.set_playlist_binding(Some(binding));
}
```

### Séquence corrigée

```
1. source_snapshot = get_renderer_queue_snapshot(source)
2. clear_renderer_queue(dest)
3. dest.replace_queue(source_snapshot.items, current_index)  // Queue remplie OK
4. dest.set_playlist_binding(binding avec pending_refresh=false)  // Binding transféré OK
5. play() sur destination avec bonne queue OK
```

### Analyse des événements

L'analyse a confirmé que les émissions d'événements étaient correctes :

| Méthode | Événement émis |
|---------|----------------|
| `replace_queue()` | `QueueUpdated` |
| `enqueue_items()` | `QueueUpdated` |
| `clear_queue()` | `QueueUpdated` |
| `set_playlist_binding()` | `BindingChanged` |
| `clear_playlist_binding()` | `BindingChanged` |

Le problème de lenteur UI était lié au fait que la queue était écrasée puis re-remplie, causant plusieurs événements successifs et une confusion dans l'état affiché.

### Fichier modifié

- `pmocontrol/src/control_point.rs` : Modification de `transfer_queue()` lignes 1223-1244

---

**Statut** : Corrigé et testé
