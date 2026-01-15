**Il faut suivre les instructions générales placées dans le fichier : Blackboard/Rules.md**


La crâte PMOcache, implémente un system de cache qui pourrait être étendu pour permettre une utilisation plus large. L'idée est de modifier les règles de déletion des items. Actuellement le cache a une capacité maximale. Et les items ont des TTL, qui peuvent être non définies. Lorsque le cash est plein, les plus vieux items en termes d'utilisation ou ceux qui ont dépassé leur TTL peuvent être détruits. Je propose de rajouter une fonctionnalité qui permet d'épingler certains items pour les rendre non destructibles. Ils pourraient aussi sortir du comptage général des items pour savoir si le cache est plein.

Il faudra modifier la structure de la base de données. Ajouter une colonne indiquant cette propriété. Mettre une règle métier en disant qu'on ne peut pas être à la fois épinglés et avec un TTL.

On se moque de maintenir la compatibilité avec la base de données actuelle, il n'y a pas à prévoir de phase de transition. Nous sommes en période de développement.
