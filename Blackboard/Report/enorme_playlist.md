# Rapport : Optimisation performance OpenHome playlist

## Résumé
Les optimizations implementées réduisent significativement le temps de synchronisation des playlists OpenHome de ~1000 titres. Les principales améliorations : passage du batch ReadList de 64 à 256 (−75% appels SOAP), elimination des doubles appels queue_snapshot() (−50% appels SOAP), et introduction du polling adaptatif avec intervalle long en veille (5s vs 500ms).

## Fichiers modifies

1. `pmocontrol/src/queue/openhome.rs`
   - Batch ReadList augmente de 64 a 256
   - Signature de replace_queue_with_pivot et replace_queue_standard_lcs modifiee pour accepter snapshot et current_track_ids
   - Appel a sync_queue mis a jour pour passer les donnees deja disponibles
   -Nouvelle fonction lcs_flags_optimized avec elimination pre/suffixe communs

2. `pmocontrol/src/music_renderer/watcher.rs`
   - Ajout du champ is_active dans WatchedState pour le polling adaptatif

3. `pmocontrol/src/music_renderer/musicrenderer.rs`
   - Boucle watcher avec intervalle adaptatif (500ms actif, 5000ms veille)
   - Marqueurs is_active=true dans play(), stop(), seek_rel_time(), sync_queue()
