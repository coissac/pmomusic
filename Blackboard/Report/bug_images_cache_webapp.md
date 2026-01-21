# Rapport : Correction du bug d'affichage des images en cache dans la webapp

## Tâche originale

Les images de couverture d'album ne s'affichaient plus dans l'application web Control Point. Seuls les logos SVG de fallback étaient visibles, bien que les URLs soient correctes et les images accessibles.

## Analyse

### Pattern d'affichage existant

Les composants utilisaient le pattern suivant :

```vue
<img
    v-if="item.album_art_uri && !imageError"
    v-show="imageLoaded"
    :src="item.album_art_uri"
    @load="handleImageLoad"
    @error="handleImageError"
/>
<div v-if="!item.album_art_uri || imageError || !imageLoaded" class="placeholder">
    <Music :size="20" />
</div>
```

Avec :
```typescript
const imageLoaded = ref(false);

watch(() => props.item.album_art_uri, () => {
    imageLoaded.value = false;
    imageError.value = false;
});

function handleImageLoad() {
    imageLoaded.value = true;
}
```

### Cause du bug

Lorsqu'une image est **déjà en cache du navigateur**, elle peut se charger de manière **synchrone** avant que Vue n'ait attaché l'écouteur d'événement `@load`. Dans ce cas :

1. L'image est créée dans le DOM (via `v-if`)
2. Le navigateur charge l'image immédiatement depuis le cache
3. L'événement `load` se déclenche **avant** que Vue n'ait attaché `@load`
4. `imageLoaded` reste à `false`
5. L'image reste cachée par `v-show="imageLoaded"`
6. Le placeholder SVG s'affiche à la place

Ce comportement est particulièrement fréquent avec des images déjà visitées ou après un rechargement de page.

## Correction appliquée

### Solution

Ajout d'une fonction `checkImageComplete()` qui vérifie si l'image est déjà chargée via les propriétés natives de l'élément `<img>` :

```typescript
const coverImageRef = ref<HTMLImageElement | null>(null);

function checkImageComplete() {
    nextTick(() => {
        if (coverImageRef.value?.complete && coverImageRef.value?.naturalWidth > 0) {
            imageLoaded.value = true;
            imageError.value = false;
        }
    });
}

onMounted(() => {
    checkImageComplete();
});

watch(() => metadata.value?.album_art_uri, (newUri) => {
    imageLoaded.value = false;
    imageError.value = false;
    if (newUri) {
        checkImageComplete();
    }
});
```

Et ajout de la référence sur l'élément `<img>` :

```vue
<img ref="coverImageRef" ... />
```

### Fichiers modifiés

| Fichier | Modification |
|---------|--------------|
| `pmoapp/webapp/src/components/pmocontrol/CurrentTrack.vue` | Ajout `checkImageComplete()`, `coverImageRef`, `onMounted` |
| `pmoapp/webapp/src/components/pmocontrol/QueueItem.vue` | Ajout `checkImageComplete()`, `coverImageRef`, `onMounted` |
| `pmoapp/webapp/src/components/pmocontrol/MediaItem.vue` | Ajout `checkImageComplete()`, `coverImageRef`, `onMounted` |
| `pmoapp/webapp/src/components/pmocontrol/RendererCard.vue` | Ajout complet de la gestion d'état image (était absent) |

### Détail des modifications par composant

#### CurrentTrack.vue
- Import de `onMounted`, `nextTick`
- Ajout de `coverImageRef`
- Ajout de `checkImageComplete()`
- Modification du `watch` pour appeler `checkImageComplete()` après changement d'URL
- Ajout de `onMounted(() => checkImageComplete())`
- Ajout de `ref="coverImageRef"` sur l'élément `<img>`

#### QueueItem.vue
- Mêmes modifications que CurrentTrack.vue

#### MediaItem.vue
- Mêmes modifications que CurrentTrack.vue

#### RendererCard.vue
- Ce composant n'avait pas de gestion d'état de chargement d'image
- Ajout complet : `imageLoaded`, `imageError`, `coverImageRef`, `checkImageComplete()`
- Ajout des handlers `@load` et `@error`
- Modification de `hasCover` pour inclure `!imageError`
- Ajout de `v-show="imageLoaded"` sur l'image
- Modification de la condition du placeholder

## Vérification

- Build webpack réussi sans erreur
- Compilation TypeScript OK

## Remarque technique

La propriété `HTMLImageElement.complete` retourne `true` si :
- L'image a fini de charger (succès ou erreur)
- L'attribut `src` est vide ou absent

C'est pourquoi on vérifie également `naturalWidth > 0` pour s'assurer que l'image a bien été chargée avec succès (une image en erreur a `naturalWidth === 0`).
