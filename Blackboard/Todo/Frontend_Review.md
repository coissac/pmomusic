** Ce travail devra être réalisé en suivant scrupuleusement les consignes listées dans le fichier [@Rules_optimal.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules_optimal.md) **

## Contexte

Ce document est le résultat d'une revue de code complète du frontend Vue.js/TypeScript du Control Point
(`pmoapp/webapp/src/`). L'application est fonctionnelle mais présente plusieurs classes de problèmes
qui peuvent causer des fuites mémoire, des incohérences de réactivité Vue 3, et des difficultés de
maintenance à mesure que l'app grandit.

**Périmètre** : uniquement `pmoapp/webapp/src/` (composants, services, composables, stores, utils, CSS).

---

## Problèmes identifiés

### P0 — Fuites mémoire via listeners SSE jamais nettoyés

**Fichiers** : `src/composables/useSSE.ts`, `src/composables/useRenderers.ts`,
`src/composables/useMediaServers.ts`

Les abonnements aux événements SSE sont créés lors du premier appel de chaque composable, mais
jamais nettoyés si le composable est réutilisé ou le composant détruit. Dans `useSSE.ts`, la
fonction `onRendererEvent()` retourne une fonction de cleanup, mais `useRendererEvents()` ignore
ce retour — l'abonnement reste actif indefiniment.

De plus, dans `imageCache.ts`, un `setInterval` de cleanup s'exécute toutes les 5 minutes sans
jamais être annulé si l'app est détruite.

### P1 — Réactivité Vue incohérente avec `shallowRef` + Maps

**Fichier** : `src/composables/useRenderers.ts` (L23-37)

`snapshots` et `loadingIds` sont déclarés en `shallowRef<Map<...>>()`. Vue ne détecte pas les
mutations d'objets à l'intérieur d'un `shallowRef`. La solution actuelle — `triggerSnapshotReactivity()`
qui crée une nouvelle Map à chaque appel — force une re-render complète de tous les composants
qui dépendent de `snapshots`, même si seul un renderer a changé.

### P2 — Race condition à l'initialisation (main.ts)

**Fichier** : `src/main.ts`

Le UIStore est initialisé après le montage de l'app et l'appel à `sse.connect()`. Des événements
SSE peuvent arriver avant que `useUIStore()` soit appelé dans les composants, et les notifications
correspondantes peuvent être perdues.

### P3 — SSE singleton sans garantie formelle

**Fichiers** : `src/composables/useRenderers.ts`, `src/composables/useMediaServers.ts`

Chaque composable maintient son propre flag `sseInitialized` pour éviter les double-abonnements.
Le mécanisme repose sur une convention implicite fragile : si deux composables s'abonnent au même
type d'événement SSE dans des contextes différents, les callbacks s'accumulent sans être
dédupliqués.

Dans `useSSE.ts`, `setupConnectionListener()` vérifie `connectionCallbacks.size === 0` mais
sans lock — deux appels simultanés peuvent installer deux listeners.

### P4 — Pas de timeout ni retry sur les requêtes `fetch`

**Fichier** : `src/services/pmocontrol/api.ts`

Toutes les requêtes `fetch()` sont émises sans `AbortController`. Si le serveur ne répond pas,
la promesse pend indéfiniment, bloquant potentiellement les composants qui attendent le résultat.
Il n'y a ni timeout configurable ni retry automatique au niveau du service.

### P5 — Validation absente des réponses API

**Fichiers** : `src/services/audioCache.ts` (L76), `src/services/coverCache.ts`,
`src/services/playlists.ts`, `src/services/pmocontrol/api.ts`

Les réponses JSON sont acceptées sans vérification de structure. Une assertion de type comme
`metadata as { origin_url?: unknown }` ne protège pas contre un changement d'API côté Rust. Si
l'API retourne une structure inattendue, le crash survient au runtime, pas à la compilation.

### P6 — Type assertions dangereuses dans PMOPlayer

**Fichier** : `src/services/pmosource.ts` / PMOPlayer (L197-214)

Les messages de commande sont typés `Record<string, unknown>`, puis les propriétés sont castées
directement : `msg.url as string`, `msg.timestamp as number`. Si une propriété est absente ou
d'un type différent, TypeScript ne le détecte pas.

### P7 — `useTabs` : watch multiples sans debounce, flag de restauration non-réinitialisé

**Fichier** : `src/composables/useTabs.ts` (L44, L350-361)

Trois `watch()` séparées écrivent dans `localStorage`. Sans debounce commun, si 3 onglets
changent d'état simultanément, `localStorage` est écrit 3 fois de suite.

Le flag `isRestoringFromStorage` (L44) empêche la boucle de sauvegarde pendant la restauration,
mais sans timeout : si `restoreFromLocalStorage()` lance une exception non-catchée, le flag reste
`true` et toutes les sauvegardes futures sont silencieusement ignorées.

### P8 — Routes de debug exposées en production, pas de lazy loading

**Fichier** : `src/router/index.ts`

Les routes debug (CoversCache, AudioCache, UPnP Explorer, etc.) sont accessibles en production
sans contrôle d'accès. Par ailleurs, tous les composants sont importés statiquement, augmentant
le bundle initial inutilement — les vues debug notamment ne sont jamais utilisées en prod.

### P9 — `formatMsToShortTime` est un alias inutile

**Fichier** : `src/utils/time.ts` (L58-59)

```typescript
// Actuellement
export function formatMsToShortTime(ms: number | null): string {
    return formatMsToTime(ms);
}
```

Fonction identique à `formatMsToTime`. Tous les appelants peuvent utiliser directement
`formatMsToTime`.

### P10 — `truncate()` dans `string.ts` peut dépasser `maxLength`

**Fichier** : `src/utils/string.ts` (L46-48)

```typescript
// Actuellement
export function truncate(str: string, maxLength: number, suffix = '…'): string {
    return str.length > maxLength ? str.slice(0, maxLength - suffix.length) + suffix : str;
}
```

Si `suffix.length >= maxLength`, `str.slice(0, maxLength - suffix.length)` retourne une chaîne
de longueur négative (comportement silencieux en JS, retourne `''`), et le résultat final est
plus long que `maxLength`.

### P11 — `DEFAULT_COVER_SVG` inline dans coverCache.ts

**Fichier** : `src/services/coverCache.ts` (L206-226)

Un SVG inline de ~20 lignes est inclus dans chaque bundle qui importe `coverCache`. Il devrait
être un fichier `src/assets/default-cover.svg` importé nativement par Vite (ce qui permet le
tree-shaking et le caching HTTP séparé).

### P12 — `animations` CSS sans `prefers-reduced-motion`

**Fichiers** : `src/assets/styles/glass-theme.css` (L384-401),
`src/assets/styles/pmocontrol.css` (L82)

Les animations `glassShimmer` (2s infini) et le `pulse` du badge de statut `Transitioning`
s'exécutent sans tenir compte de `prefers-reduced-motion: reduce`. Sur certains systèmes ou
pour des utilisateurs sensibles au mouvement, ces animations sont gênantes.

### P13 — CSS dupliqué dans drawers.css

**Fichier** : `src/assets/styles/drawers.css` (L105-149)

`drawer-close-btn` et `drawer-back-btn` partagent 90% des styles. Un TODO présent en L167
("remplacer par la classe globale .section-title") confirme cette dette. La variable
`var(--opacity-disabled)` est utilisée mais non définie dans `variables.css`.

### P14 — `browseContainer` : clés de cache fragiles et pas de pagination

**Fichier** : `src/composables/useMediaServers.ts` (L145, L239)

Les clés de cache sont construites comme `${serverId}/${containerId}`. Si un `containerId`
contient un slash (séparateur d'URL), la clé est ambigüe. Par exemple, `server1/a/b` peut
correspondre à serverId=`server1`, containerId=`a/b` ou serverId=`server1/a`, containerId=`b`.

La pagination n'est pas implémentée côté composable : `browseContainer` charge toujours
offset=0, limit=50. Pour les containers avec 500+ items, les items au-delà de 50 ne sont
jamais accessibles.

### P15 — Notifications sans limite de taille dans `ui.ts`

**Fichier** : `src/stores/ui.ts` (L50, L55-57)

Un bug ou une boucle d'erreur peut générer des centaines de notifications. Le tableau
`notifications` n'est pas limité. Chaque notification crée un `setTimeout` individuel, et
si le store est détruit avant l'expiration, ces callbacks persistent (ghosts).

---

## Plan d'exécution

Les corrections sont groupées par effort et impact. Les P0–P3 concernent la fiabilité
(fuites mémoire, réactivité), les P4–P8 la robustesse et maintenabilité, les P9–P15 la
qualité et la dette technique.

### Étape 1 — Corriger les fuites mémoire SSE (P0)

Dans `useSSE.ts`, stocker et appeler les fonctions de cleanup retournées par `onRendererEvent` /
`onMediaServerEvent` :

```typescript
// useSSE.ts – useRendererEvents()
onMounted(() => {
    const cleanup = onRendererEvent(rendererId(), handler);
    onUnmounted(cleanup);  // ← actuellement ignoré
});
```

Dans `imageCache.ts`, exporter une fonction `destroyImageCache()` qui appelle `clearInterval`
sur le timer de cleanup, et l'appeler dans le `onUnmounted` de l'app root.

### Étape 2 — Stabiliser la réactivité des snapshots (P1)

Remplacer `shallowRef<Map<...>>` + `triggerSnapshotReactivity` par `reactive(new Map<...>)`.
Vue 3 rend les Maps réactives nativement. Les composants qui lisent `snapshots.get(id)`
seront notifiés uniquement si ce `id` change.

```typescript
// Avant
const snapshots = shallowRef<Map<string, FullRendererSnapshot>>(new Map());
function triggerSnapshotReactivity() {
    snapshots.value = new Map(snapshots.value);
}

// Après
const snapshots = reactive(new Map<string, FullRendererSnapshot>());
// Les modifications directes (snapshots.set/delete) déclenchent la réactivité
```

### Étape 3 — Timeout fetch + AbortController (P4)

Ajouter un helper dans `api.ts` :

```typescript
function fetchWithTimeout(url: string, options?: RequestInit, timeoutMs = 10_000): Promise<Response> {
    const controller = new AbortController();
    const id = setTimeout(() => controller.abort(), timeoutMs);
    return fetch(url, { ...options, signal: controller.signal })
        .finally(() => clearTimeout(id));
}
```

Utiliser `fetchWithTimeout` pour toutes les requêtes dans le service API.

### Étape 4 — Corriger `useTabs` watchs et flag de restauration (P7)

Fusionner les trois `watch()` en un seul `watchEffect` avec un debounce unique (100ms).
Encadrer `isRestoringFromStorage` dans un bloc `try/finally` :

```typescript
async function restoreFromLocalStorage() {
    isRestoringFromStorage = true;
    try {
        // ... logique de restauration
    } catch (e) {
        console.error('Tab restore failed:', e);
    } finally {
        isRestoringFromStorage = false;
    }
}
```

### Étape 5 — Limit de notifications et nettoyage timers (P15)

```typescript
const MAX_NOTIFICATIONS = 5;

function addNotification(notif: Omit<Notification, 'id'>): void {
    if (notifications.value.length >= MAX_NOTIFICATIONS) {
        notifications.value.shift(); // supprimer la plus ancienne
    }
    const id = nextId++;
    const timer = setTimeout(() => removeNotification(id), notif.duration ?? 5000);
    notificationTimers.set(id, timer);
    notifications.value.push({ ...notif, id });
}

function $dispose() {
    notificationTimers.forEach(clearTimeout);
    notificationTimers.clear();
}
```

### Étape 6 — Lazy loading des routes et protection debug (P8)

```typescript
// router/index.ts
const DebugView = () => import('../views/DebugView.vue');
const isDev = import.meta.env.DEV;

const routes = [
    // ... routes normales
    ...(isDev ? [{ path: '/debug', component: DebugView }] : []),
    { path: '/:pathMatch(.*)*', redirect: '/' }, // wildcard 404
];
```

### Étape 7 — Corrections mineures (P9, P10, P11, P12, P13)

- **P9** : Supprimer `formatMsToShortTime`, remplacer tous les appels par `formatMsToTime`
- **P10** : Ajouter un guard dans `truncate` : `if (suffix.length >= maxLength) return str.slice(0, maxLength)`
- **P11** : Déplacer le SVG dans `src/assets/default-cover.svg` et l'importer avec `import defaultCover from '../assets/default-cover.svg?raw'`
- **P12** : Entourer les animations CSS avec `@media (prefers-reduced-motion: no-preference) { ... }`
- **P13** : Factoriser `drawer-close-btn` / `drawer-back-btn` avec une classe `.drawer-icon-btn`. Définir `--opacity-disabled: 0.4` dans `variables.css`

### Étape 8 — Clés de cache et pagination (P14)

Encoder les IDs dans les clés de cache :

```typescript
const cacheKey = `${encodeURIComponent(serverId)}:${encodeURIComponent(containerId)}`;
```

Utiliser `:` comme séparateur (absent de l'encoding) pour éviter toute ambigüité.

Pour la pagination, ajouter une propriété `hasMore: boolean` et `loadMore()` au résultat de
`browseContainer`, incrementant offset à chaque appel.

### Ordre d'exécution

1. Étape 1 — fuites SSE (P0) — fiabilité critique
2. Étape 2 — réactivité Map (P1) — fiabilité
3. Étape 3 — timeout fetch (P4) — robustesse réseau
4. Étape 4 — useTabs (P7) — fiabilité des onglets
5. Étape 5 — notifications (P15) — stabilité UI
6. Étape 6 — router (P8) — sécurité + performance bundle
7. Étape 7 — corrections mineures (P9–P13)
8. Étape 8 — cache keys + pagination (P14)

## Règle après ces corrections

**Interdit** : créer un abonnement SSE (`onRendererEvent`, `onMediaServerEvent`) sans stocker
et appeler la fonction de cleanup retournée dans `onUnmounted`.

**Interdit** : utiliser `shallowRef<Map<...>>` avec mutation directe — utiliser `reactive(new Map())`
pour les Maps qui doivent déclencher la réactivité Vue sur leurs entrées.

**Obligatoire** : toute requête `fetch()` dans un service doit utiliser `fetchWithTimeout`
avec un AbortController.
