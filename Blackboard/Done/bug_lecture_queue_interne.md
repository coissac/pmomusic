# Bug lecture queue interne - RESOLU

## Tâche originale

**Crate concernée** : pmocontrol

**Problème rapporté** : Lors de la lecture sur un Renderer avec queue interne, si l'utilisateur clique sur un item de la queue pour déclencher sa lecture, tout semble se passer normalement pendant une seconde. Puis, avant que la lecture ne démarre réellement, le lecteur passe à la piste suivante.

---

## Synthèse de la résolution

### Cause racine

Race condition dans la logique d'auto-advance du watcher. Quand l'utilisateur sélectionne une piste :
1. Les commandes UPnP `SetAVTransportURI` + `Play` sont envoyées
2. Le renderer passe brièvement par un état `STOPPED` pendant l'initialisation
3. Le watcher détecte ce `STOPPED` et déclenche l'auto-advance vers la piste suivante

Le système ne distinguait pas un état `STOPPED` transitoire (initialisation) d'un état `STOPPED` réel (fin de piste).

### Solution

Ajout d'un flag `has_played_since_track_start` dans `MusicRendererState` :

- **Remis à `false`** au démarrage d'une nouvelle piste (`play_from_index`, `play_from_queue`, etc.) et lors d'un `stop()`
- **Passé à `true`** quand l'état `PLAYING` est détecté par le watcher
- **L'auto-advance n'est autorisé** que si le flag est `true`

Ainsi, un état `STOPPED` transitoire (avant que `PLAYING` ne soit observé) n'entraîne plus d'auto-advance.

### Fichier modifié

- `pmocontrol/src/music_renderer/musicrenderer.rs`

### Méthodes ajoutées/modifiées

- `MusicRendererState.has_played_since_track_start` (nouveau champ)
- `set_has_played_flag()`, `clear_has_played_flag()`, `check_and_clear_has_played_flag()` (nouvelles méthodes)
- `handle_state_change()` (modifié pour utiliser le flag)
- `play_current_from_queue()`, `play_next_from_queue()`, `play_from_index()`, `play_from_queue()`, `stop()` (modifiés pour réinitialiser le flag)

---

**Statut** : Corrigé et testé
