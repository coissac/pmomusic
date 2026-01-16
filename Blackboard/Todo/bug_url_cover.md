**Il faut suivre les instructions générales placées dans le fichier : Blackboard/Rules.md**

## Crate concernée
- **pmoplaylist**
- **pmocache**
- **pmoaudiocache**
- **pmocover**
- **pmodidl**

Le bug doit se situer dans la crâte PMO Playlist. Les autres crates ne sont cités car elles doivent être utilisées par PMO Playlist pour cette fonctionnalité.

## Description du bug

Le document didl Généré par les PMO playlists, Possède une URL absolue pour le flux audio, Mais relative pour l'URL de la cover. Les deux entités flux audio et cover sont stockées dans des caches PMO audio cache et PMO Cover respectivement.

Il faut comprendre comment est construite l'URL absolue du flux audio et appliquer la même recette à l'URL de la cover.
