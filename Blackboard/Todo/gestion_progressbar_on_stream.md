** Ce travail devra être réalisé en suivant scrupuleusement les consignes listées dans le fichier [@Rules.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules.md) **

- **La crâte concernée est [@pmocontrol](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmocontrol)** SEULEMENT celle-ci pourra être modifiée.

**Objectif de la tâche**: fournir une gestion correcte de la barre de progression de lecture sur des flux continu type radio artificiellement segmentée par l'intermédiaire des métadonnées

Les radios web émettent un flux continu de données. Certaines d'entre elles émettent en parallèle des métadonnées permettant d'un point de vue logique de segmenter ce flux continu en chunks auxquels correspondent des métadonnées différentes. Il s'agit avec ce patch de faire en sorte que la Progress Bar reflète l'état d'avancement à l'intérieur de chacun de ces segments virtuels.

**Méthode proposée**: Patcher la gestion des événements SSE vers l'application web de manière à envoyer des données de position de lecture en accord avec ces métadonnées dans le cas d'emissions en flux continu.

**Étape 1**: Implementer au niveau de la classe `MusicRenderer` une méthode predicat `is_playing_a_stream` qui retourne `true` si la lecture est en cours et que la musique est une radio en flux continu, et `false` sinon. Pour construire cela Une méthode adaptée à chacun des `MusicRendererBackend` est nécessaire. 

sur les lecteurs UPNP ou Chromecast ou assimilés. En fait tous les lecteurs qui ne gèrent pas en interne une playlist de lecture, cette implémentation est triviale. Lorsque l'on joue un morceau, on sait quel morceau est en train de jouer, il suffit donc de vérifier que le flux est délimité dans le temps ou pas. Cela peut se faire au moment où l'on envoie l'ordre de lecture d'une URL. On vérifie cette URL et l'on place un flag continous_stream dans le backend correspondant à vrai.

Pour les rendereurs OpenHome Il me semble que le plus simple est de modifier la méthode `playback_position` de [@openhome.rs](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmocontrol/src/music_renderer/openhome.rs) check si l'URL en train d'être jouée a changé. Si c'est le cas, on vérifie si la nouvelle URL est une radio en flux continu et on met à jour le flag continous_stream.

**Étape 2**: Afin de tester cette fonctionnalité correctement et pour rendre l'interface w&eb plus informative. Il faudrait pousser via l'interface SSE une information indiquant le changement d'état : Flux continu vs morceau. dans la même idée on peut rajouter à l'API REST du controlpoint Un point indiquant si l'on est en train de jouer un morceau ou un flux.

On pourra alors modifier l'appli web [@webapp](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoapp/webapp) en vue Pour qu'elle affiche à côté de l'emplacement où est marquée `Attachée à une playlist` Un petit flag du même type graphique indiquant que l'on est sur une web radio.
