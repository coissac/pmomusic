# Rapport : Correction du bug de lecture sur queue interne

## Problème

Lors de la lecture sur un Renderer avec queue interne, si l'utilisateur clique sur un item de la queue pour déclencher sa lecture, tout semble se passer normalement pendant une seconde. Puis, avant que la lecture ne démarre réellement, le lecteur passe à la piste suivante.

## Analyse

### Cause identifiée

Le problème était une **race condition** dans la logique d'auto-advance du watcher.

Quand l'utilisateur clique sur un item de la queue :
1. `play_queue_index` est appelé dans `ControlPoint`
2. Les commandes UPnP `SetAVTransportURI` + `Play` sont envoyées au renderer
3. Le renderer peut passer brièvement par un état `STOPPED` pendant l'initialisation de la nouvelle piste
4. Le watcher (polling toutes les 500ms) détecte cet état `STOPPED`
5. Comme la lecture était lancée depuis la queue (`PlaybackSource::FromQueue`), l'auto-advance se déclenche et passe à la piste suivante

### Détail technique

La logique d'auto-advance dans `handle_state_change` vérifie si `is_playing_from_queue()` retourne `true` pour décider de passer à la piste suivante quand l'état `STOPPED` est détecté. Cependant, il n'y avait aucun mécanisme pour distinguer :
- Un état `STOPPED` transitoire pendant l'initialisation d'une nouvelle piste
- Un état `STOPPED` réel indiquant la fin de lecture d'une piste

## Solution implémentée

Ajout d'un flag `has_played_since_track_start` dans `MusicRendererState` qui permet de tracker si l'état `PLAYING` a été observé depuis le dernier démarrage de piste.

### Logique du flag

1. **Quand on démarre une nouvelle piste** (`play_from_index`, `play_from_queue`, `play_next_from_queue`, `play_current_from_queue`) : le flag est remis à `false`

2. **Quand le watcher détecte l'état `PLAYING`** : le flag passe à `true`

3. **Quand le watcher détecte l'état `STOPPED`** :
   - Si `has_played_since_track_start == true` : c'est une vraie fin de piste → auto-advance autorisé
   - Si `has_played_since_track_start == false` : c'est un état transitoire pendant l'initialisation → auto-advance bloqué

4. **Quand `stop()` est appelé** : le flag est remis à `false`

## Fichiers modifiés

### `pmocontrol/src/music_renderer/musicrenderer.rs`

1. **Ajout du champ `has_played_since_track_start`** dans `MusicRendererState` :
```rust
struct MusicRendererState {
    // ...
    /// Flag indicating that a PLAYING state has been observed since the last track start.
    /// This prevents auto-advance on transient STOPPED states during track initialization.
    /// Auto-advance is only allowed when this flag is true.
    has_played_since_track_start: bool,
}
```

2. **Ajout des méthodes de gestion du flag** :
   - `set_has_played_flag()` : met le flag à `true`
   - `clear_has_played_flag()` : met le flag à `false` (publique)
   - `check_and_clear_has_played_flag()` : vérifie et remet à `false`

3. **Modification de `handle_state_change`** :
   - Sur `PLAYING` : appelle `set_has_played_flag()`
   - Sur `STOPPED` avec `is_playing_from_queue()` : vérifie `check_and_clear_has_played_flag()` avant d'auto-advance

4. **Modification des méthodes de démarrage de lecture** :
   - `play_current_from_queue()`
   - `play_next_from_queue()`
   - `play_from_index()`
   - `play_from_queue()`
   - `stop()`
   
   Toutes appellent `clear_has_played_flag()` pour réinitialiser le flag.

## Tests effectués

- Clic sur différents items de la queue : la piste sélectionnée est bien jouée sans saut
- Lecture normale jusqu'à la fin d'une piste : l'auto-advance vers la piste suivante fonctionne correctement
- Arrêt manuel (stop) : pas d'auto-advance intempestif
