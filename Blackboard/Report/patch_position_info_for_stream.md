# Rapport : Patch des informations de position pour les flux continus

## Résumé
Modification de la méthode `poll_and_emit_changes()` dans la crate `pmocontrol` pour corriger les données de position et durée lorsqu'un renderer diffuse un flux continu (webradio). La méthode détecte maintenant si un flux est en cours via `is_playing_a_stream()` et applique un traitement spécifique : extraction de la durée depuis les métadonnées DIDL, et calcul de la position relative depuis `track_start_time` (qui est déjà maintenu à jour lors des changements de métadonnées). Si aucune durée n'est disponible, la position et la durée sont mises à zéro/none.

## Fichiers modifiés

1. `pmocontrol/src/music_renderer/musicrenderer.rs`
   - Modification de la méthode `poll_and_emit_changes()` pour patcher les informations de position lors de la détection d'un flux continu
   - Ajout du logging au niveau info lors de la détection d'un flux continu
   - Extraction conditionnelle de la durée depuis les métadonnées DIDL pour les streams
   - Calcul de la position relative basé sur `track_start_time` (différence entre now et track_start_time)
   - Retour de valeurs par défaut (zéro pour position, none pour duration) si aucune durée n'est disponible dans les métadonnées
   - Préservation de la logique existante pour les médias réguliers (non-streams)
   - Note : `track_start_time` est déjà maintenu à jour par la logique existante lors des changements de métadonnées
