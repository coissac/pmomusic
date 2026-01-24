**Tu réaliseras ce travail en appliquant scrupuleusement les règles définies dans [@Rules.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules.md)**

Ce travail se réalisera dans la crate [@pmoradiofrance](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoradiofrance).


- L'ensemble des éditions devraient se réaliser dans le fichier [@playlist.rs](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoradiofrance/src/playlist.rs).
- Il est fort probable que tu n'aies besoin de lire aucun autre fichier.

Le problème, les groupes de radio sont composés d'une radio principale, parfois, et de web radio parfois.

Quand on a une radio principale unique, comme France Culture, tout se passe bien.

Quand on a un groupe de radio avec une radio principale et des web radios, la radio principale devrait se trouver à l'index 0 du container et les radios secondaires aux index suivants. Mais la radio principale est absente. ÷Sans doute qu'elle a été écrasée par des webradio.

Et quant au seul cas de groupe de radio qui n'ont pas de radio principale, ICI, Les radios locales de Radio France, Rien ne marche du tout, on n'arrive même pas à accéder au container. C'est peut-être un reliquat du temps où tu traitais ICI à part.
