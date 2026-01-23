**Tu réaliseras ce travail en appliquant scrupuleusement les règles définies dans [@Rules.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules.md)**

**On ne travaille que dans la Crate PMORadioFrance**

À partir de maintenant, tu ne prends plus en compte ce que tu pensais avant et tu écoutes bien. Et tu construis un plan d'implémentation que je dois valider. Tu arrêtes de prendre des initiatives et de faire des bêtises.

- Tu as des fonctions d'interrogation de l'API Radio France qu'il faut utiliser au minimum. Mais Radio France nous donne des dates d'invalidation des métadonnées. Globalement, on doit gérer une grosse map où les valeurs ont des TTL. 
- Quand un client demande une donnée du cache, si le TTL est atteint, il commence par utiliser l'API Radio France, modifie le cache, puis la retourne. Dans le cas contraire, il retourne directement la donnée.

A chaque fois qu'il fait un appel de l'API Radio France pour modifier ses valeurs, Le cache émet un événement ou avertit ses abonnés, comme quoi les données d'un slug particulier ont été modifiées. Comme ça tout le monde peut se synchroniser.

Les clients, par exemple la fonction Browse, n'interrogent que le cache qui a forcément des données à jour. Les métadonnées ne sont jamais stockées hors du cache, on se réfère toujours à elles.

Nous devons maintenant considérer le fonctionnement du control point. Celui-ci est capable de s'abonner à une playlist pour suivre ses modifications. Il ne peut pas s'abonner à un item. 

Dans le cas d'une radio, on peut considérer que chaque canal, chaque slug, est en réalité une playlist à un item qu'il faut suivre. Ainsi, le Control Point peut décider de jouer cette playlist en s'abonnant à elle et être tenu au courant des modifications par des événements GENA.

La source Radio France doit donc s'abonner aux événements du Cache. A chaque fois qu'un slug est modifié, elle avertit par un événement Jenna que la playlist à un item qui correspond à ce Slug est modifiée.

Maintenant, il y a le cache des stations. Le cache des stations finalement il ne stock qu'un emboîtement de listes de slug. Ça, normalement, ça ne bouge quasiment pas. On peut dire que une fois par jour, on met à jour ce cache. Les listes de slug ont donc un TTL mais très long. 

A chaque browse, on reconstruit un document didl à partir des métadonnées à jour provenant du cache.
