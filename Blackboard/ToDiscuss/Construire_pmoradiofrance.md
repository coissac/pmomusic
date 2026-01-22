** Tu dois suivre scrupuleusement les règles définies dans le fichier [@Rules.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules.md) **


** Cette tâche n'est pas une tâche de codage, c'est une tâche de réflexion. Elle doit conduire à la rédaction d'un rapport dans le répertoire [@ToThinkAbout](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/ToThinkAbout) **

à parir de la page [web](https://www.radiofrance.fr/franceculture) peux-tu comprendre comment elle obtient les informations prsésenté ci-dessous:

```html
<div role="heading" aria-level="1" slot="title" class="CoverRadio-title qg-tt3 svelte-1thibul"><!----><span class="truncate qg-focus-container svelte-1t7i9vq"><!----><a href="/franceculture/podcasts/le-journal-de-l-eco/le-jouet-profite-de-la-morosite-ambiante-4949584" aria-label="Le Journal de l'éco • Le jouet profite de la morosité ambiante" data-testid="link" class="svelte-1t7i9vq underline-hover"><!----><!---->Le Journal de l'éco • Le jouet profite de la morosité ambiante<!----></a><!----></span><!----></div>
```

et

```html
<p class="CoverRadio-subtitle qg-tt5 qg-focus-container svelte-1thibul" slot="subtitle"><!----><!----><!----><a href="/franceculture/podcasts/les-matins" data-testid="link" class="svelte-1t7i9vq"><!---->Les Matins<!----></a><!----> <span class="CoverRadio-producer qg-tx1 svelte-qz676b">par Guillaume Erner</span><!----><!----></p>
```

## Round 2:

À partir de [@api_radiofrance_complete.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/ToThinkAbout/api_radiofrance_complete.md) et de [@music_source.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Architecture/music_source.md) 

Nous allons commencer l'implémentation de pmoradiofrance en implémentant dans un fichier client.rs Une API de requêtes sur Radio France. Pour les fonctionnalités, il faut effectivement se reporter au fichier `api_radiofrance_complete.md` Et pour l'architecture, il est possible de s'inspirer de [@client.rs](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoparadise/src/client.rs)

## Round 3

A partir des découvertes faites durant le round 2, Tu as maintenant le droit d'écrire du code. Tu peux donc écrire le fichier: client.rs de la nouvelle crate pmoradiofrance. Pour cela, tu devras t'appuyer sur les rapports [@api_radiofrance_complete.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/ToThinkAbout/api_radiofrance_complete.md) et de [@music_source.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Architecture/music_source.md) 

Il faut absolument cacher les réponses de l'API, Afin de limiter au maximum les requêtes inutiles, Notamment, on sait que les chaînes et les web radios ne changent que très rarement. On peut peut-être penser à faire une extension de configuration [@pmoconfig_ext.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Architecture/pmoconfig_ext.md) Pour stocker les informations principales ainsi qu'un timestamp et se dire que sauf requêtes forcées on ne va pas mettre à jour cette liste plus d'une fois par semaine.

## Round 4

Je pense qu'il faut maintenant implémenter le deuxième niveau de la Crate Radio France, En implémentant un client Stateful qui peut retourner facilement des listes de radio, des playlists, qui charge ces informations soit à part de la configuration, soit à partir de l'API, suivant que le TTL est dépassé ou pas. Le tout pour préparer la construction de la source Radio France.

Pensez comme rêgle métier à renommer les choses qui apparaissent comme `France Bleue` en `ICI` Au niveau des labels d'affichage, pas des slugs évidemment.

Voilà le type d'arborescence de browsing qu'on pourrait avoir.
On ne présente à chaque fois qu'un lien vers le flux de plus haute résolution.


```
Radio France
├── France Culture
├── FIP
│   ├── FIP
│   ├── FIP Cultes
│   ├── FIP Nouveautes
│   ├── FIP ...
│   └── FIP Pop
├── Mouv'
├── ...
└── ICI
    ├── ICI Alsace
    ├── ICI Armorique
    ├── ICI Auxerre 
    ├── ... 
    └── ICI Vaucluse
```

Sous le folder principal Radio France, On doit avoir une playlist par station. Les stations qui n'ont qu'une seule chaîne ne contiennent que cette chaîne dans leur PMOplaylist, Les autres, si elles ont une station principale comme FIP, commencent par cette station principale puis leur station annexe.
Évidemment, normalement, le contenu en titre des playlists ne doit pas évoluer. Mais les métadonnées si régulièrement Si l'on met le titre de la station comme équivalent d'un titre d'album, Le nom de l'émission pourrait être l'auteur. Et le titre de l'émission du jour, le titre du morceau. Ainsi, au fur et à mesure du temps, on fait évoluer des métadonnées pour changer la couverture, le nom de l'émission, le titre de l'émissions du jour.

Je pense que les playlists doivent être des playlists volatiles. Les covers doivent être cachés dans [@pmocovers](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmocovers) Par contre les URL doivent être passées telles qu'elles, C'est du pur stream, on ne va pas les cacher dans le PMOaudiocache.

étend le document de réflexion pour proposer une architecture: [@api_radiofrance_complete.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/ToThinkAbout/api_radiofrance_complete.md)
