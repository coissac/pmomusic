** Ce travail devra être réalisé en suivant scrupuleusement les consignes listées dans le fichier [@Rules_optimal.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules_optimal.md) **

## Contexte et symptôme

Le frontend Vue.js du control point PMOMusic (`pmoapp/webapp`) présente des problèmes de
performance significatifs avec des playlists de ~1000 titres. La plupart des lenteurs sont
côté UI, indépendamment des optimisations déjà réalisées côté Rust/backend.

Les trois manifestations observées :
- **Freeze au scroll** dans la file d'attente (QueueViewer) dès ~200 items
- **Blocage UI temporaire** à l'ouverture d'une playlist dans PlayListManager
- **Refetches JSON répétés** déclenchés par les événements SSE `queue_updated`

## Causes racines identifiées

### P0 — Pas de virtualisation dans `QueueViewer.vue`

**Fichier** : `src/components/pmocontrol/QueueViewer.vue:85-93`

```vue
<!-- Tous les items rendus en DOM simultanément -->
<div v-if="queue?.items.length" class="queue-list" ref="queueContainer">
    <QueueItem
        v-for="item in queue.items"
        :key="item.index"
        ...
    />
</div>
```

Pour 1 000 titres : 1 000 nœuds DOM permanents, chacun contenant une image, un composable
réactif (`useCoverImage`), et des computed properties. Le scroll devient impossible.

**Ironie** : `vue-virtual-scroller@^2.0.0-beta.8` est présent dans `package.json` mais
**n'est utilisé nulle part** dans la codebase — manifestement prévu puis abandonné.

### P1 — Pas de virtualisation dans `PlayListManager.vue`

**Fichier** : `src/components/PlayListManager.vue:562-724`

```vue
<div class="track-grid" v-if="sortedTracks.length > 0">
    <article v-for="track in sortedTracks" :key="..." class="track-card">
        <!-- image + title + artist + album + durée + bitrate + samplerate -->
    </article>
</div>
```

Même problème : grid CSS avec 1 000 articles et leurs images (`loading="lazy"`).

Aggravé par la computed `sortedTracks` (ligne 883-890) :
```typescript
const sortedTracks = computed(() => {
    return [...detail.tracks].sort(   // copie complète du tableau
        (a, b) => new Date(b.added_at).getTime() - new Date(a.added_at).getTime()
    );
});
```
Et `lazyTracksCount` (ligne 892-897) qui filtre les 1 000 items à chaque re-render.

### P2 — `queue_updated` force un refetch JSON complet

**Fichier** : `src/composables/useRenderers.ts:200-206`

```typescript
case "queue_updated":
    snapshot.state.queue_len = event.queue_length;
    queueRefreshingIds.delete(rendererId);
    // Pour la queue complète, on doit refetch
    void fetchRendererSnapshot(rendererId, { force: true });
    break;
```

`fetchRendererSnapshot()` appelle `api.getRendererFullSnapshot(rendererId)` qui retourne
le snapshot complet incluant **tous les items de la queue avec leurs métadonnées**.

Pour 1 000 titres, le payload JSON peut atteindre plusieurs centaines de Ko. Si l'utilisateur
charge une playlist de 1 000 titres depuis un serveur qui émet les items par batch (ex. 64
par 64 côté Rust), l'événement `queue_updated` est émis plusieurs fois de suite, déclenchant
autant de refetches consécutifs du même JSON complet.

**Note** : `loadingIds.has(rendererId)` (ligne 354) déduplique les requêtes simultanées,
mais pas les requêtes consécutives rapprochées.

### P3 — Infinite scroll accumule toutes les pages en mémoire (`MediaBrowser`)

**Fichier** : `src/composables/useMediaServers.ts:200-204`

```typescript
// Accumuler les nouvelles entrées — jamais purgées
state.entries.push(...data.entries)
```

En parcourant un serveur contenant 1 000 titres, toutes les pages de 50 items
s'accumulent dans `browseCache` sans jamais être libérées. Résultat : après un scroll
complet, 1 000 entrées sont en mémoire ET en DOM simultanément.

### P4 — `scrollIntoView` sur 1 000 nœuds DOM non virtualisés

**Fichier** : `src/components/pmocontrol/QueueViewer.vue:27-48`

```typescript
watch(() => queue.value?.current_index, async (currentIndex) => {
    await nextTick();
    const currentItem = queueContainer.value.querySelector(".queue-item.current");
    if (currentItem) {
        currentItem.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }
}, { immediate: true });
```

`querySelector` sur un conteneur de 1 000 nœuds + animation CSS `smooth` provoque un
layout thrashing. Ce watcher est aussi déclenché au montage (`immediate: true`), ce qui
peut provoquer un re-layout au chargement initial de la page.

## Ce qui fonctionne déjà correctement

- **Déduplication des snapshots simultanés** : `loadingIds.has(rendererId)` évite les
  requêtes parallèles pour le même renderer — à préserver.
- **`loading="lazy"` sur les images** dans PlayListManager — efficace une fois que le DOM
  est virtualisé.
- **Cache du browse** avec invalidation par conteneur SSE — architecture correcte.
- **`useCoverImage`** avec retry exponentiel et cleanup — à conserver tel quel dans
  `QueueItem.vue`.
- **Connexion SSE unique** partagée entre tous les composants — bonne architecture.

## Plan d'exécution

### Répertoire concerné : `pmoapp/webapp`

---

### Étape 1 — Virtualiser la file d'attente dans `QueueViewer.vue`

**Fichier** : `src/components/pmocontrol/QueueViewer.vue`

`vue-virtual-scroller` est déjà installé. Remplacer le `v-for` nu par `<RecycleScroller>` :

```vue
<script setup lang="ts">
import { RecycleScroller } from 'vue-virtual-scroller';
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css';
// ... imports existants inchangés
</script>

<template>
    <!-- Remplacer le div.queue-list + v-for par : -->
    <RecycleScroller
        v-if="queue?.items.length"
        class="queue-list"
        :items="queue.items"
        :item-size="64"
        key-field="index"
        v-slot="{ item }"
        ref="queueContainer"
    >
        <QueueItem
            :item="item"
            :is-current="item.index === queue.current_index"
            @click="handleItemClick"
        />
    </RecycleScroller>
</template>
```

`item-size="64"` correspond à la hauteur CSS actuelle de `.queue-item` (padding +
cover 48px + gap). À ajuster si le CSS change.

**Adapter `scrollIntoView`** : `RecycleScroller` expose une méthode `scrollToItem(index)`.
Remplacer le `querySelector` + `scrollIntoView` par :

```typescript
watch(() => queue.value?.current_index, async (currentIndex) => {
    if (currentIndex !== null && currentIndex !== undefined && queueContainer.value) {
        await nextTick();
        queueContainer.value.scrollToItem(currentIndex);
    }
}, { immediate: true });
```

**Fonctionnalité préservée** : `QueueItem.vue` reste inchangé — `RecycleScroller` recycle
les nœuds DOM au lieu de les créer tous, mais les props passées à chaque item sont
identiques.

---

### Étape 2 — Débouncer les refetches `queue_updated`

**Fichier** : `src/composables/useRenderers.ts:200-206`

Le problème : `queue_updated` arrive N fois de suite pendant le chargement d'une grande
playlist, déclenchant N refetches.

Ajouter un debounce par renderer sur l'appel à `fetchRendererSnapshot` :

```typescript
// Map des timers de debounce par renderer (à déclarer en module scope)
const queueUpdateDebounceTimers = new Map<string, ReturnType<typeof setTimeout>>();
const QUEUE_UPDATE_DEBOUNCE_MS = 300;

// Dans le case "queue_updated" :
case "queue_updated":
    snapshot.state.queue_len = event.queue_length;
    queueRefreshingIds.delete(rendererId);

    // Annuler le timer précédent pour ce renderer
    const existingTimer = queueUpdateDebounceTimers.get(rendererId);
    if (existingTimer) clearTimeout(existingTimer);

    // Programmer un seul fetch après stabilisation
    queueUpdateDebounceTimers.set(rendererId, setTimeout(() => {
        queueUpdateDebounceTimers.delete(rendererId);
        void fetchRendererSnapshot(rendererId, { force: true });
    }, QUEUE_UPDATE_DEBOUNCE_MS));
    break;
```

**Fonctionnalité préservée** : Si un seul `queue_updated` arrive (cas normal), le refetch
est simplement retardé de 300 ms — imperceptible. Si N arrivent en rafale (chargement
d'une grande playlist), un seul refetch est déclenché à la fin.

**Contrainte** : Ne pas dépasser 500 ms de debounce — l'indicateur `queueRefreshing` dans
l'UI doit se désactiver rapidement après la fin du chargement.

---

### Étape 3 — Virtualiser la grille dans `PlayListManager.vue`

**Fichier** : `src/components/PlayListManager.vue`

La grille CSS ne peut pas être virtualisée directement avec `RecycleScroller` (liste 1D).
Remplacer la grid par une liste virtualisée, ou introduire une pagination côté client :

```typescript
const PAGE_SIZE = 100;
const currentPage = ref(0);

const paginatedTracks = computed(() =>
    sortedTracks.value.slice(
        currentPage.value * PAGE_SIZE,
        (currentPage.value + 1) * PAGE_SIZE
    )
);
```

Avec des boutons de navigation Précédent / Suivant et un indicateur de page.

**Optimiser `sortedTracks`** : mémoriser le résultat par `playlist.id` pour éviter
la copie+tri à chaque re-render non lié à la playlist :

```typescript
const sortedTracksCache = new Map<string, TrackEntry[]>();

const sortedTracks = computed(() => {
    const detail = selectedPlaylist.value;
    if (!detail) return [];
    const cached = sortedTracksCache.get(detail.id);
    if (cached && cached.length === detail.tracks.length) return cached;
    const sorted = [...detail.tracks].sort(
        (a, b) => new Date(b.added_at).getTime() - new Date(a.added_at).getTime()
    );
    sortedTracksCache.set(detail.id, sorted);
    return sorted;
});
```

**Simplifier `lazyTracksCount`** : le dériver de `sortedTracks` pour ne pas parcourir
le tableau original en parallèle :

```typescript
const lazyTracksCount = computed(() =>
    sortedTracks.value.filter(isLazyTrack).length
);
```

---

### Étape 4 — Limiter l'accumulation dans le browse infini (`MediaBrowser`)

**Fichier** : `src/composables/useMediaServers.ts:183-212`

Implémenter une fenêtre glissante dans `browseCache` : conserver seulement les 200
derniers items en mémoire :

```typescript
const BROWSE_WINDOW_SIZE = 200;

async function loadMoreBrowse(serverId: string, containerId: string) {
    // ... code existant jusqu'à la récupération de data ...

    // Remplacer : state.entries.push(...data.entries)
    // Par :
    const combined = [...state.entries, ...data.entries];
    state.entries = combined.slice(-BROWSE_WINDOW_SIZE);
    state.total_count = data.total_count;
    state.currentOffset = (state.currentOffset ?? 0) + data.entries.length;
    state.hasMore = state.currentOffset < state.total_count;
    browseCache.value.set(key, { ...state });
}
```

**Invariant à préserver** : `state.currentOffset` et `state.hasMore` doivent continuer
de refléter la position réelle dans la liste serveur, indépendamment de ce qui est
affiché — leur logique ne change pas.

---

## Ordre d'exécution

1. **Étape 1** — Virtualisation `QueueViewer` (impact le plus visible, composant le plus simple)
2. **Étape 2** — Debounce `queue_updated` (élimine les refetches en cascade, changement minimal)
3. **Étape 3** — Optimisation `PlayListManager` (plus complexe, composant de 2039 lignes)
4. **Étape 4** — Fenêtre glissante `MediaBrowser` (amélioration mémoire, moins critique)

## Périmètre : ce qui ne change pas

- `QueueItem.vue` : aucune modification (recycling géré par le parent)
- `useCoverImage.ts` : aucune modification (lazy loading + retry déjà corrects)
- `useSSE.ts` : aucune modification (connexion unique, bonne architecture)
- `loadingIds` dans `fetchRendererSnapshot` : déduplication conservée
- `api.getRendererFullSnapshot` : le payload reste complet, pas de pagination API
- Tous les événements SSE autres que `queue_updated` : aucune modification

## Tests recommandés

Demander à l'humain de :

```bash
cd pmoapp/webapp
npm run dev
```

Puis tester manuellement :
- Ouvrir la file d'attente d'un renderer OpenHome avec 1 000 titres : scroll fluide ?
- Vérifier que la piste courante est visible au changement de piste (`scrollToItem`)
- Charger une playlist de 1 000 titres via PlayListManager : absence de blocage ?
- Observer les requêtes réseau dans DevTools lors du chargement d'une grande playlist :
  un seul `GET /renderers/{id}/full` doit être émis après la fin du chargement
