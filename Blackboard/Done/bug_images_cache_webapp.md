# Bug : Images non affichées dans la webapp (fallback SVG systématique)

## Description du bug

Dans l'application web Control Point, les images de couverture d'album ne s'affichent plus correctement. Au lieu d'afficher les images, seuls les petits logos SVG de fallback (icône Music) sont visibles, alors que :
- Les URLs des images sont correctes
- Les images sont bien présentes dans le cache
- Les images sont accessibles via leur URL directe

Ce bug est apparu après une correction précédente visant à éliminer les images grises.

## Symptômes

- Les composants affichent l'icône SVG de fallback au lieu des vraies images
- Le problème est plus fréquent qu'avant la correction précédente
- Les images en cache du navigateur ne s'affichent pas

## Crates/Modules concernées

- **pmoapp/webapp** (application Vue.js)

## Composants à examiner

- `src/components/pmocontrol/CurrentTrack.vue`
- `src/components/pmocontrol/QueueItem.vue`
- `src/components/pmocontrol/MediaItem.vue`
- `src/components/pmocontrol/RendererCard.vue`

## Cause suspectée

Le pattern d'affichage d'image avec `v-show="imageLoaded"` ne gère pas correctement le cas où l'image est déjà en cache du navigateur. Dans ce cas, l'événement `@load` peut se déclencher de manière synchrone avant que Vue n'ait attaché l'écouteur, laissant `imageLoaded` à `false`.

## Solution attendue

Ajouter une vérification de l'état `complete` de l'image après le montage du composant et après chaque changement d'URL, pour détecter les images déjà chargées depuis le cache.
