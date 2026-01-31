** Ce travail devra être réalisé en suivant scrupuleusement les consignes listées dans le fichier [@Rules_optimal.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules_optimal.md) **

Cette modification cible la cratte pmocontrol uniquement

Les musiques Renderer, Disposent maintenant d'une méthode leur permettant de savoir s'ils sont en train de diffuser une webradio via leur méthode is_playing_a_stream.

Il faut donc que dans la méthode poll_and_emit_changes On fait ce qui est nécessaire pour envoyer des données de position et de durée de track corrigée si l'on a is_playing_a_stream à vrai.

Si is_playing_a_stream à vrai:
  - Maintenir à jour la valeur `track_start_time` de la classe 
    MusicRenderer En la mettant égale à now de metadata.
  - Loguer cet événement au niveau info.
  - Extraire la durée du morceau depuis les métadonnées 
    fournies par la structure de position.
    - Si la durée est disponible:
      - Utilisez cette donnée pour la pousser sur le bus des événements.
      - calculer la position dans le flux comme la différence 
        entre now et track_start_time.
    - Sinon: Envoyer zéro pour la position et none pour la duration.
- Sinon, transmettre les données fournies comme actuellement.
