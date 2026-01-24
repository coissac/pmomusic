# Rapport : Correction du bug d'update des covers dans l'interface web

## Résumé

Tentative de correction du problème de mise à jour des images de couverture dans l'application web PMOMusic. Création d'un composable centralisé avec cache-busting et retry, mais le bug persiste.

## Solution implémentée

### 1. Création d'un composable réutilisable

**Fichier créé** : `pmoapp/webapp/src/composables/useCoverImage.ts`

Ce nouveau composable centralise toute la logique de chargement d'images avec les fonctionnalités suivantes :

- **Retry automatique** : Jusqu'à 3 tentatives de rechargement en cas d'erreur
- **Backoff exponentiel** : Délai croissant entre chaque retry (1s, 2s, 3s)
- **Cache busting** : Ajout de paramètres timestamp pour forcer le rechargement
- **Gestion d'état robuste** : Suivi de l'état de chargement, erreur, et nombre de retries
- **Détection du cache** : Vérification si l'image est déjà chargée (images en cache)
- **Logging** : Messages de debug pour faciliter le débogage

**Interface du composable** :

```typescript
export interface CoverImageOptions {
    maxRetries?: number;      // Défaut: 3
    retryDelay?: number;      // Défaut: 1000ms
    forceReload?: boolean;    // Défaut: true
}

export function useCoverImage(
    imageUrl: Ref<string | null | undefined>,
    options?: CoverImageOptions
)
```

**Retour** :
```typescript
{
    imageLoaded: Ref<boolean>,
    imageError: Ref<boolean>,
    coverImageRef: Ref<HTMLImageElement | null>,
    handleImageLoad: Function,
    handleImageError: Function
}
```

### 2. Refactorisation des composants

Tous les composants utilisant des images de couverture ont été refactorisés pour utiliser le nouveau composable :

**Fichiers modifiés** :
1. `pmoapp/webapp/src/components/pmocontrol/CurrentTrack.vue`
2. `pmoapp/webapp/src/components/pmocontrol/MediaItem.vue`
3. `pmoapp/webapp/src/components/pmocontrol/QueueItem.vue`
4. `pmoapp/webapp/src/components/pmocontrol/RendererCard.vue`
5. `pmoapp/webapp/src/components/pmocontrol/ContainerItem.vue`

**Changements effectués dans chaque composant** :

- Suppression du code de gestion d'image dupliqué (watch, onMounted, checkImageComplete, etc.)
- Remplacement par un simple appel au composable `useCoverImage`
- Réduction du code de 40-60 lignes à environ 3 lignes

**Avant** :
```typescript
const imageLoaded = ref(false);
const imageError = ref(false);
const coverImageRef = ref<HTMLImageElement | null>(null);

function checkImageComplete() { /* ... */ }
watch(() => metadata.value?.album_art_uri, /* ... */);
onMounted(() => { /* ... */ });
function handleImageLoad() { /* ... */ }
function handleImageError() { /* ... */ }
```

**Après** :
```typescript
const albumArtUri = computed(() => metadata.value?.album_art_uri);
const { imageLoaded, imageError, coverImageRef, handleImageLoad, handleImageError } =
    useCoverImage(albumArtUri);
```

## Avantages de cette solution

1. **Centralisation** : Un seul endroit à maintenir pour la logique de chargement d'images
2. **Robustesse** : Retry automatique en cas d'erreur réseau ou de timing
3. **Debugging** : Logs détaillés pour identifier les problèmes
4. **Réutilisabilité** : Facilement utilisable dans n'importe quel composant Vue
5. **Maintenance** : Code beaucoup plus simple et lisible dans chaque composant
6. **Cache busting** : Force le rechargement des images même si le navigateur les a en cache

## Fonctionnement technique

Le composable résout le problème principal de la façon suivante :

1. **Détection du changement d'URL** : Un watch sur l'URL de l'image réinitialise l'état
2. **Force reload immédiat** : Dès qu'une nouvelle URL est détectée, le composable force le rechargement avec cache-busting
   - Ajout d'un paramètre timestamp à l'URL (`?_cb=timestamp_r0`)
   - Mise à jour directe du `src` de l'élément `<img>`
3. **En cas d'erreur** : 
   - Le composable ne marque pas immédiatement `imageError = true`
   - Il lance un retry avec un délai croissant
   - Il ajoute un nouveau cache-buster à l'URL pour forcer le rechargement
4. **Après max retries** : Seulement alors, `imageError` est mis à true et le placeholder s'affiche

**Point clé** : Le cache-busting est appliqué **dès le premier chargement** (pas seulement en cas d'erreur), ce qui garantit que le navigateur ne réutilise pas une ancienne image en cache quand l'URL des métadonnées change.

## Tests suggérés

Pour valider la correction :

1. Démarrer l'application web
2. Jouer une track avec une cover
3. Passer à une autre track avec une cover différente
4. Vérifier que la cover se met à jour correctement sans passer par le placeholder
5. Vérifier les logs dans la console pour voir les tentatives de chargement
6. Tester avec une connexion réseau lente pour vérifier le mécanisme de retry

## Notes

- Le composable utilise un retry avec backoff exponentiel pour éviter de surcharger le serveur
- Les logs peuvent être désactivés en production en retirant les `console.log`
- Le paramètre `forceReload` peut être désactivé si le cache busting pose problème
- Le nombre de retries et le délai sont configurables via les options

## Fichiers concernés

### Créés
- `pmoapp/webapp/src/composables/useCoverImage.ts`

### Modifiés
- `pmoapp/webapp/src/composables/useCoverImage.ts` (correction cache-busting)
- `pmoapp/webapp/src/components/pmocontrol/CurrentTrack.vue`
- `pmoapp/webapp/src/components/pmocontrol/MediaItem.vue`
- `pmoapp/webapp/src/components/pmocontrol/QueueItem.vue`
- `pmoapp/webapp/src/components/pmocontrol/RendererCard.vue`
- `pmoapp/webapp/src/components/pmocontrol/ContainerItem.vue`
