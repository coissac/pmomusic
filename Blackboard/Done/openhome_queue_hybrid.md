**Tu réaliseras ce travail en appliquant scrupuleusement les règles définies dans [@Rules.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules.md)**


Nous allons travailler spécifiquement et sur rien d'autre que la queue Open Home des Média Renderer dans la CRAT PMO Control. [@openhome.rs](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmocontrol/src/queue/openhome.rs) 

Tu ne peux modifier que ce fichier et a priori tu n'as besoin de lire que ce fichier.

Actuellement, cette queue est Stateless. C'est parfait, sauf sur un point, la gestion des métadonnées. En effet, les services OpenHome ne permettent pas de modifier les métadonnées d'une piste. Et cela m'ennuie. car mon control point ne peut pas mettre à jour les métadonnées d'une piste si elles sont changées par le média serveur

Les items de la queue openhome sont identifiés par un ID. L'idée est de maintenir en cache dans la structure de queue Open Home une map qui lit cette ID avec des métadonnées. Je parle bien de l'ID open home de la track et pas de l'index (position) dans la queue de lecture. 

Tout le jeu consistera à enregistrer une copie des métadonnées dans cette map à partir de toutes les méthode du fichier [@openhome.rs](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmocontrol/src/queue/openhome.rs) Qui accepte des PlaybackItem :

- append_or_init_index
- replace_item
- sync_queue

Inversement, à chaque fois qu'on retournera un playback item, On n'oubliera pas de renvoyer les métadonnées du cache plutôt que celles renvoyées par OpenHome. Peut-être qu'il est juste nécessaire de modifier playback_item_from_entry

On profitera des appels réguliers à la fonction queue_snapshot Pour faire le ménage dans le cache en ne gardant que les entrées qui correspondent aux ID de la queue actuelle.

Cela nous permettra de rajouter, une fonction d'update des métadonnées d'un item de la queue. Au niveau du backend open home, puis dans un second temps des autres backend de queue, puis du Média Renderer.
