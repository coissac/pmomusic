** Ce travail devra être réalisé en suivant scrupuleusement les consignes listées dans le fichier [@Rules_optimal.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules_optimal.md) **

## Vue d'ensemble

Le frontend de PMOMusic est une application Vue 3 + TypeScript avec Pinia, organisée autour
de composables réactifs, d'un client SSE centralisé et d'un cache API à plusieurs niveaux.
L'architecture générale est solide : séparation claire composables/services/vues, typage strict
(`strict: true` dans `tsconfig.app.json`), reconnexion SSE avec backoff exponentiel.

Cette revue documente les bugs avérés, les fragilités de conception et les axes d'amélioration
relevés lors d'une lecture complète des fichiers `pmoapp/webapp/src/`.

---

## Bugs

### 1. `apiCache.ts:130-133` — Mutation globale du TTL non réentrante

**Problème** : la méthode `fetch()` accepte un `ttl` optionnel par appel. Pour l'appliquer,
elle modifie `this.options.ttl` globalement avant d'appeler `this.set()`, puis le restaure :

```typescript
// apiCache.ts:130-133
if (ttl) {
  const originalTtl = this.options.ttl;
  this.options.ttl = ttl;          // (A) modification globale
  this.set(endpoint, data, params);
  this.options.ttl = originalTtl;  // (B) restauration
}
```

**Cause** : `fetcher()` est `await`-é (ligne 128) avant ce bloc. Pendant cet await, d'autres
microtasks peuvent s'intercaler et appeler `isFresh()` ou `set()`, qui lisent `this.options.ttl`.
Si deux appels `fetch()` avec des `ttl` différents sont en vol simultanément, la restauration
de (B) peut effacer la valeur posée par le second appel concurrent, ou (A) peut lire un TTL
modifié par un autre appel.

**Solution** : passer le `ttl` directement à `set()` comme paramètre, sans modifier l'état
partagé :

```typescript
// Dans set() : ajouter un paramètre ttl optionnel
set<T>(endpoint: string, data: T, params?: ..., etag?: string, ttl?: number): void {
  const key = this.makeKey(endpoint, params);
  this.cache.set(key, {
    data,
    timestamp: Date.now(),
    ttl: ttl ?? this.options.ttl,  // TTL par entrée, pas global
    etag,
  });
  this.notifySubscribers(key, data);
}

// Dans isFresh() : lire le ttl de l'entrée
private isFresh(key: string): boolean {
  const entry = this.cache.get(key);
  if (!entry) return false;
  return Date.now() - entry.timestamp < (entry.ttl ?? this.options.ttl);
}

// Dans fetch() : supprimer le bloc de mutation globale
this.set(endpoint, data, params, undefined, ttl);
```

---

### 2. `useRenderers.ts:151 + 222` — Réassignation post-switch écrase le nouvel objet

**Problème** : le handler `onRendererEvent` termine par une ligne inconditionnelle :

```typescript
// useRenderers.ts:222
snapshotState.snapshots.set(rendererId, snapshot);
```

Cette ligne s'exécute pour **tous** les types d'événements après le `switch`, y compris pour
`position_changed` et `metadata_changed` qui ont déjà créé et stocké un nouvel objet dans le
Map à l'intérieur du switch :

```typescript
// position_changed — ligne 151 : stocke newSnapshot
snapshotState.snapshots.set(rendererId, newSnapshot);
break;
// → puis ligne 222 écrase avec snapshot (proxy d'origine)

// metadata_changed — ligne 176 : stocke un spread
snapshotState.snapshots.set(rendererId, { ...snapshot, state: { ...snapshot.state } });
break;
// → puis ligne 222 écrase avec snapshot (proxy d'origine)
```

**Cause** : `break` sort du `switch` mais pas de la fonction. La ligne 222 est atteinte dans
tous les cas.

**Conséquence** : les objets créés pour forcer la détection de changement par Vue sont
immédiatement écrasés. Le mécanisme de réactivité fonctionne malgré tout (le proxy muté est
re-stocké), mais la logique est trompeuse et fragile : si Vue venait à optimiser la détection
d'identité des objets réactifs, cette redondance deviendrait un bug visible.

**Solution** : supprimer la ligne 222 et s'assurer que chaque branche du switch stocke
explicitement son résultat dans la Map. Les branches `volume_changed`, `mute_changed` et
`binding_changed` qui mutent directement `snapshot` doivent aussi créer un nouvel objet :

```typescript
case "volume_changed":
  snapshotState.snapshots.set(rendererId, {
    ...snapshot,
    state: { ...snapshot.state, volume: event.volume },
  });
  break;

case "mute_changed":
  snapshotState.snapshots.set(rendererId, {
    ...snapshot,
    state: { ...snapshot.state, mute: event.mute },
  });
  break;
// idem pour binding_changed, stream_state_changed
```

Supprimer la ligne 222. Chaque case devient responsable de son stockage, ce qui élimine aussi
le besoin de `toRaw()`.

---

### 3. `useRenderers.ts:122` — `as any` sur `transport_state`

**Problème** :

```typescript
// useRenderers.ts:122
snapshot.state.transport_state = event.state as any;
```

**Cause** : `event.state` est typé `string` (type SSE générique), alors que
`transport_state` est une union littérale (`"PLAYING" | "PAUSED" | "STOPPED" | ...`).
Le cast `as any` contourne la vérification de type.

**Conséquence** : si le backend envoie une valeur non prévue (ex. `"TRANSITIONING"`), elle
sera stockée sans validation. Les composants qui comparent `transport_state === "PLAYING"`
ne matcheront pas et l'UI restera muette.

**Solution** : définir un guard de type ou une assertion dans `types.ts` :

```typescript
// services/pmocontrol/types.ts
export type TransportState = "PLAYING" | "PAUSED" | "STOPPED" | "NO_MEDIA" | "TRANSITIONING";

export function isTransportState(s: string): s is TransportState {
  return ["PLAYING", "PAUSED", "STOPPED", "NO_MEDIA", "TRANSITIONING"].includes(s);
}
```

```typescript
// useRenderers.ts — case state_changed
case "state_changed":
  if (isTransportState(event.state)) {
    snapshot.state.transport_state = event.state;
  } else {
    console.warn(`[useRenderers] transport_state inconnu: ${event.state}`);
  }
  break;
```

---

### 4. `apiCache.ts:181-200` — `invalidate()` supprime silencieusement les subscriptions actives

**Problème** :

```typescript
// apiCache.ts:197-200
keysToDelete.forEach(key => {
  this.cache.delete(key);
  this.subscriptions.delete(key);  // ← subscriptions perdues sans notification
});
```

**Cause** : lors d'une invalidation (ex. après un SSE event), les callbacks enregistrés via
`subscribe()` sont supprimés de la Map. Les composants ne sont pas notifiés de la suppression
et ne reçoivent plus les futures mises à jour même après un refetch.

**Conséquence** : un composant qui a appelé `apiCache.subscribe(...)` et qui survit à une
invalidation devient « sourd » sans le savoir.

**Solution** : conserver les subscriptions lors d'une invalidation — seule la donnée en cache
est périmée, pas les abonnés :

```typescript
invalidate(pattern: string): void {
  const keysToDelete: string[] = [];
  // ... construction de keysToDelete inchangée ...
  keysToDelete.forEach(key => {
    this.cache.delete(key);
    // NE PAS supprimer this.subscriptions.get(key)
    // Les abonnés seront notifiés lors du prochain set()
  });
}
```

Si l'on veut notifier les abonnés d'une invalidation (pour qu'ils affichent un état de
chargement), ajouter un callback optionnel `onInvalidate` dans l'interface de subscription.

---

## Fragilités de conception

### 5. `useRenderers.ts:7,143-151` — `toRaw()` comme contournement de réactivité Vue

**Problème** : le code utilise `toRaw()` pour extraire l'objet brut d'un proxy Vue avant de
faire un spread, afin que la copie ne contienne pas de getters réactifs qui pointent vers
l'objet original :

```typescript
// useRenderers.ts:143-151
const rawState = toRaw(snapshot.state);
const newState = { ...rawState };
const newSnapshot = { ...snapshot, state: newState };
snapshotState.snapshots.set(rendererId, newSnapshot);
```

**Cause** : les Maps imbriquées dans un objet `reactive()` ont un comportement de réactivité
peu prévisible dans Vue 3. Vue ne détecte pas les mutations d'éléments d'une Map réactive si
la référence de la Map elle-même ne change pas.

**Solution recommandée** : remplacer `reactive(new Map())` par `shallowRef(new Map())` pour
les Maps qui contiennent des données complexes. La réactivité se déclenche en remplaçant la
Map entière (ou en forçant un `triggerRef`) :

```typescript
// Au lieu de :
const snapshotState = reactive<RendererSnapshotState>({ snapshots: reactive(new Map()), ... });

// Utiliser :
const snapshots = shallowRef(new Map<string, FullRendererSnapshot>());

// Pour déclencher la réactivité après mutation :
snapshots.value = new Map(snapshots.value);  // ou triggerRef(snapshots)
```

Cela rend la propagation de réactivité explicite et élimine le besoin de `toRaw()`.

---

### 6. `useRenderers.ts:552-570` — Timer de debounce non nettoyé dans `useRenderer()`

**Problème** : `useRenderer()` crée un timer de debounce local qui n'est jamais nettoyé si
le composant parent est démonté :

```typescript
// useRenderers.ts:553-566
let refreshDebounceTimer: ReturnType<typeof setTimeout> | null = null;
const REFRESH_DEBOUNCE_MS = 500;

async function refresh(force = true) {
  if (refreshDebounceTimer !== null) return;
  refreshDebounceTimer = setTimeout(() => {
    refreshDebounceTimer = null;
  }, REFRESH_DEBOUNCE_MS);
  await Promise.all([...]);
}
```

**Cause** : pas d'appel à `clearTimeout` dans un `onUnmounted`. Si le composant est démonté
pendant les 500 ms du debounce, le timer continue de s'exécuter.

**Conséquence** : fuite mémoire potentielle ; dans des cas extrêmes (navigation rapide), le
callback peut tenter de déclencher un fetch sur un composant déjà démonté.

**Solution** :

```typescript
import { onUnmounted } from 'vue';

// Dans useRenderer() :
onUnmounted(() => {
  if (refreshDebounceTimer !== null) {
    clearTimeout(refreshDebounceTimer);
    refreshDebounceTimer = null;
  }
});
```

---

### 7. `useRenderers.ts:45-46` — Singleton SSE initialisé par flag de module non réinitialisable

**Problème** :

```typescript
// useRenderers.ts:45-46
let sseInitialized = false;
function ensureSSEInitialized() {
  if (sseInitialized) return;
  // ...
  sseInitialized = true;
}
```

**Cause** : ce flag de module est persistant pour toute la durée de vie de la page. Si la
connexion SSE est perdue puis rétablie avec un nouvel objet `PMOControlSSE`, le handler
`onRendererEvent` précédent peut ne plus être actif, mais `sseInitialized` empêche sa
re-enregistration.

**Conséquence** : après une déconnexion et reconnexion SSE, les événements renderer peuvent
ne plus être reçus par `useRenderers` jusqu'à un rechargement de page.

**Solution** : exposer une fonction `resetSSE()` qui remet `sseInitialized = false` et la
connecter à l'événement de reconnexion du service SSE. Alternativement, utiliser le pattern
`provide/inject` ou un store Pinia pour gérer le cycle de vie SSE explicitement, en lieu et
place du flag de module.

---

## Qualité du code

### 8. `useTabs.ts` — Deep watch déclenchant une sérialisation localStorage à chaque mutation

**Problème** : le watch qui persiste l'état des onglets utilise `{ deep: true }` sur un
tableau qui peut contenir jusqu'à 12 entrées avec des métadonnées :

```typescript
// useTabs.ts:349-355
watch(
  () => [state.tabs, state.activeTabId, state.tabHistory],
  () => { saveToLocalStorage(); },
  { deep: true },
);
```

**Cause** : `{ deep: true }` traverse récursivement toutes les propriétés observées.
`saveToLocalStorage()` appelle `JSON.stringify` sur l'ensemble des tabs à chaque mutation,
même mineure (ex. changement de `activeTabId`).

**Solution** : surveiller les propriétés individuellement et sérialiser uniquement ce qui
change, ou utiliser un computed pour construire la clé de changement :

```typescript
// Watch séparés, sans deep
watch(() => state.activeTabId, saveToLocalStorage);
watch(() => state.tabHistory.length, saveToLocalStorage);
watch(
  () => state.tabs.map(t => t.id + t.type + (t.metadata?.rendererId ?? '')).join('|'),
  saveToLocalStorage,
);
```

---

### 9. `UnifiedControlView.vue:40-49` — Swipe : `clientX` final au lieu de la position initiale

**Problème** :

```typescript
// UnifiedControlView.vue:40-49
useSwipe(viewRef, {
  threshold: 50,
  onSwipeEnd(_e: TouchEvent, swipeDirection: string) {
    if (swipeDirection === "right" && !drawerOpen.value) {
      const touch = _e.changedTouches[0];
      if (touch && touch.clientX < 50) {  // ← position finale du doigt
        drawerOpen.value = true;
      }
    }
  },
});
```

**Cause** : `onSwipeEnd` reçoit l'événement `touchend`. Dans `changedTouches`, `clientX`
est la position **finale** du doigt (après le swipe), pas la position initiale. Un swipe
commençant à `x=30` et terminant à `x=150` a `clientX=150` dans `touchend` — la condition
`< 50` ne sera jamais vraie pour un swipe horizontal significatif.

**Conséquence** : le geste de swipe depuis le bord gauche ne fonctionne probablement pas
sur les appareils tactiles.

**Solution** : capturer la position initiale dans `onSwipeStart` :

```typescript
const swipeStartX = ref(0);

useSwipe(viewRef, {
  threshold: 50,
  onSwipeStart(e: TouchEvent) {
    swipeStartX.value = e.touches[0]?.clientX ?? 0;
  },
  onSwipeEnd(_e: TouchEvent, swipeDirection: string) {
    if (swipeDirection === "right" && !drawerOpen.value && swipeStartX.value < 50) {
      drawerOpen.value = true;
    }
  },
});
```

---

### 10. `api.ts:50` — Réponse JSON non validée avant le cast TypeScript

**Problème** :

```typescript
// api.ts:50
return response.json();  // retour typé T par inférence, sans validation
```

**Cause** : `response.json()` retourne `Promise<any>`. TypeScript accepte le retour car la
méthode `request<T>` promet `Promise<T>`, mais aucune validation de structure n'est effectuée.

**Conséquence** : si le backend renvoie un schéma légèrement différent (champ renommé, type
changé), le bug se manifestera loin du point d'appel avec un message cryptique. En
développement avec plusieurs instances en parallèle (`udn_prefix` différent), une requête
dirigée vers la mauvaise instance peut retourner un format inattendu.

**Solution pragmatique** : ajouter une validation légère avec un type guard pour les réponses
critiques, ou au minimum loguer la réponse brute en mode développement :

```typescript
private async request<T>(path: string, options: RequestInit = {}): Promise<T> {
  // ...
  const data = await response.json();
  if (import.meta.env.DEV && data == null) {
    console.warn(`[PMOControlAPI] Réponse vide pour ${path}`);
  }
  return data as T;
}
```

Pour les endpoints critiques (`getRendererFullSnapshot`, `getRenderers`), envisager un
schéma de validation Zod ou une assertion runtime minimale.

---

## Accessibilité et feedback utilisateur

### 11. Absence de labels ARIA sur les contrôles transport

Les composants `TransportControls.vue` et `VolumeControl.vue` contiennent des boutons
iconiques (play, pause, stop, volume) sans attributs `aria-label`. Les lecteurs d'écran
ne peuvent pas identifier la fonction de ces contrôles.

**Correction minimale** :

```html
<!-- TransportControls.vue -->
<button @click="play" aria-label="Lecture">
  <PlayIcon />
</button>
<button @click="pause" aria-label="Pause">
  <PauseIcon />
</button>
```

---

### 12. Erreurs réseau silencieuses sans feedback utilisateur

Plusieurs appels critiques sont lancés en fire-and-forget sans propagation vers l'UI :

```typescript
// useRenderers.ts:86
void fetchRenderers(true);   // erreur loggée en console uniquement

// useRenderers.ts:89
void fetchRendererSnapshot(rendererId, { force: true });  // idem
```

Le store `ui.ts` dispose d'un système de notifications toast (`addNotification`). Les erreurs
de réseau devraient y être propagées pour informer l'utilisateur :

```typescript
import { useUIStore } from '@/stores/ui';

const uiStore = useUIStore();

// Dans le handler SSE :
try {
  await fetchRenderers(true);
} catch {
  uiStore.addNotification({
    message: 'Impossible de rafraîchir la liste des renderers',
    type: 'error',
  });
}
```

---

## Points forts à conserver

Ces patterns sont bien conçus et ne doivent pas être modifiés dans les corrections ci-dessus :

- **Déduplication des requêtes en vol** (`apiCache.ts:100-112`) : évite les appels réseau
  redondants quand plusieurs composants demandent la même ressource simultanément.
- **Watch sélectif sur les IDs** (`UnifiedControlView.vue:139-150`) : calcule une clé
  synthétique `ids.join(',')` au lieu d'un deep watch sur le tableau de renderers.
- **Backoff exponentiel SSE** (`sse.ts`) : reconnexion progressive 1s→2s→4s→8s→16s→30s
  avec cap. Implémentation robuste.
- **Fetch batch contrôlé** (`useRenderers.ts:351-383`) : `fetchBatchSnapshots()` avec
  concurrence limitée (défaut 3) et délai inter-batches. Évite de saturer le réseau au
  démarrage.
- **`filterRenderers` par UDN** (`UnifiedControlView.vue:107-120`) : filtre les WebRenderers
  étrangers en comparant le UDN normalisé. Logique correcte avec gestion du préfixe `uuid:`.

---

## État d'avancement — corrections appliquées (commit 2026-04-06)

Les 12 points de la revue ont été traités. Le tableau ci-dessous récapitule ce qui a été
fait et ce qui reste à finir.

| # | Problème | État |
|---|----------|------|
| 1 | `apiCache` — TTL mutation globale | ✅ Corrigé (`ttl` par entrée dans `CacheEntry`, `isFresh()` lit `entry.ttl`) |
| 2 | `useRenderers` — double réassignation post-switch | ✅ Corrigé (chaque `case` responsable, ligne 222 supprimée) |
| 3 | `useRenderers` — `as any` sur `transport_state` | ✅ Corrigé (`isTransportState` guard dans `types.ts`) |
| 4 | `apiCache` — `invalidate()` détruisait les subscriptions | ✅ Corrigé (`this.subscriptions.delete` supprimé) |
| 5 | `useRenderers` — `toRaw()` / `reactive(Map)` fragile | ✅ Migré vers `shallowRef` + helpers `triggerSnapshotReactivity()` / `triggerLoadingReactivity()` |
| 6 | `useRenderer()` — timer debounce non nettoyé | ✅ Corrigé (`onUnmounted` + `clearTimeout`) |
| 7 | Singleton SSE non réinitialisable | ✅ Corrigé (`resetSSE()` exposé dans le retour de `useRenderers()`) |
| 8 | `useTabs` — deep watch coûteux | ✅ Corrigé (watches séparés sans `deep: true`) |
| 9 | Swipe — `clientX` final au lieu d'initial | ✅ Corrigé (`swipeStartX` capturé dans `onSwipeStart`) |
| 10 | `api.ts` — JSON non validé | ✅ Corrigé (log dev-mode pour réponse nulle) |
| 11 | Absence de labels ARIA | ✅ Corrigé (4 boutons transport + bouton mute + slider volume) |
| 12 | Erreurs réseau silencieuses | ✅ Corrigé (`uiStore.notifyError()` dans `fetchRenderers` et `fetchRendererSnapshot`) |

---

## Tâches restantes

Trois problèmes résiduels ont été introduits ou laissés lors de l'implémentation du commit
ci-dessus. Ils doivent être corrigés.

### A. `useRenderers.ts` — `state_changed` : mutation directe sans déclenchement de réactivité

**Problème** : avec la migration vers `shallowRef`, les objets dans la Map ne sont plus des
proxies Vue. La mutation directe de `snapshot.state.transport_state` ne déclenche aucune
réactivité — les composants ne se mettront pas à jour quand l'état de transport change :

```typescript
// useRenderers.ts — case state_changed (code actuel)
case "state_changed":
  if (isTransportState(event.state)) {
    snapshot.state.transport_state = event.state;  // ← mutation directe, pas de trigger
  }
  break;
```

**Solution** : créer un nouvel objet, comme pour `volume_changed` et `mute_changed` :

```typescript
case "state_changed":
  if (isTransportState(event.state)) {
    snapshots.value.set(rendererId, {
      ...snapshot,
      state: { ...snapshot.state, transport_state: event.state },
    });
  } else {
    console.warn(`[useRenderers] transport_state inconnu: ${event.state}`);
  }
  break;
```

---

### B. `useRenderers.ts` — `queue_refreshing` / `queue_updated` : mutations de `queueRefreshingIds` sans trigger

**Problème** : `queueRefreshingIds` est un `shallowRef<Set>`. Les appels `.add()` et
`.delete()` sur `.value` ne déclenchent pas la réactivité de `shallowRef` :

```typescript
// useRenderers.ts — case queue_refreshing (code actuel)
case "queue_refreshing":
  queueRefreshingIds.value.add(rendererId);   // ← pas de trigger
  break;

case "queue_updated":
  queueRefreshingIds.value.delete(rendererId); // ← pas de trigger
  break;
```

Le composable `isQueueRefreshing(id)` retourne `queueRefreshingIds.value.has(id)`. Sans
trigger, les templates qui dépendent de cette valeur ne se recalculeront pas.

**Solution** : ajouter une fonction `triggerQueueReactivity()` analogue à
`triggerLoadingReactivity()` et l'appeler après chaque mutation :

```typescript
function triggerQueueReactivity() {
  queueRefreshingIds.value = new Set(queueRefreshingIds.value);
}

// Dans le switch :
case "queue_refreshing":
  queueRefreshingIds.value.add(rendererId);
  triggerQueueReactivity();
  break;

case "queue_updated":
  snapshot.state.queue_len = event.queue_length;
  queueRefreshingIds.value.delete(rendererId);
  triggerQueueReactivity();
  void fetchRendererSnapshot(rendererId, { force: true });
  break;
```

---

### C. `useRenderers.ts` — import `toRaw` et commentaire obsolètes

**Problème** : avec `shallowRef`, les objets stockés dans `snapshots.value` sont de simples
objets JavaScript (jamais des proxies Vue). L'appel `toRaw(snapshot.state)` dans
`position_changed` est devenu un no-op, et le commentaire qui le justifie est trompeur :

```typescript
// useRenderers.ts:160-163 (commentaire et import obsolètes)
// IMPORTANT: Utiliser toRaw() pour obtenir l'objet brut non-réactif avant de copier
// sinon Vue copie les getters réactifs qui continuent à pointer vers l'objet d'origine
const rawState = toRaw(snapshot.state);
const newState = { ...rawState };
```

**Solution** : supprimer le `toRaw()`, simplifier en spread direct, retirer `toRaw` de
l'import ligne 7 :

```typescript
// Remplacer :
import { ref, shallowRef, computed, toRaw, type Ref, onUnmounted } from "vue";

// Par :
import { ref, shallowRef, computed, type Ref, onUnmounted } from "vue";

// Dans position_changed :
const newSnapshot = {
  ...snapshot,
  state: {
    ...snapshot.state,
    position_ms: positionMs ?? 0,
    duration_ms: durationMs,
  },
};
snapshots.value.set(rendererId, newSnapshot);
```

Cette simplification rend aussi le cas `position_changed` cohérent avec les autres cases
(`volume_changed`, `mute_changed`, etc.) qui construisent directement l'objet final sans
passer par une variable intermédiaire.
