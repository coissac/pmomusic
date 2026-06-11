# Bug : Navigation dans les résultats de recherche Qobuz

## Symptôme

1. On tape "Camille" dans la barre de recherche → spinner → 4 containers s'affichent : "Albums (1000+)", "Artistes (1000+)", "Titres (1000+)", "Playlists (1000+)". ✓
2. On clique sur "Artistes (1000+)" → on retombe sur les **mêmes 4 containers** au lieu de la liste des artistes. ✗
3. La breadcrumb en haut montre bien qu'on est dans "Artistes (1000+)" — donc la navigation a eu lieu, mais le contenu affiché est wrong.

## Cause racine identifiée (côté frontend)

Dans `pmoapp/webapp/src/components/pmocontrol/MediaBrowser.vue` :

```typescript
const isSearchMode = computed(() => searchQuery.value !== '');

const browseData = computed(() =>
    isSearchMode.value
        ? searchResults.value          // ← toujours ça quand on est en search mode
        : getBrowseCached(props.serverId, props.containerId),
);
```

Quand `isSearchMode` est true (après une recherche), `browseData` retourne TOUJOURS `searchResults` (les 4 groupes), peu importe le `containerId` courant. Donc cliquer sur "Artistes" change `containerId` → le watcher charge bien les artistes du serveur dans le cache → mais `browseData` ignore le cache et re-affiche `searchResults`.

Le serveur de son côté fonctionne correctement :
- Browse de `qobuz:search:catalog:artists:camille` → appelle `execute_search(Artists, "camille")` → retourne la liste des artistes
- Le log DIDL confirme que les bons artistes sont retournés

## Ce qui a été tenté (et raté)

### Tentative : stocker les résultats dans browseCache

`searchServer()` dans `useMediaServers.ts` modifié pour ne plus écrire dans `searchResults` mais directement dans `browseCache` sous la clé `search:camille`, puis naviguer vers cet ID.

Résultat : `isSearchMode` devient toujours false (searchQuery jamais set), donc `browseData` utilise `getBrowseCached`. Mais les 4 groupes n'apparaissent plus. Cause non confirmée — probablement un problème de réactivité Vue ou de timing entre le navigate et le watcher.

**État actuel du code** : ce fix a été partiellement appliqué (voir commits récents). `handleSearch` appelle encore l'ancienne `searchServer()`. Le code est dans un état incohérent — voir diff.

## Architecture correcte (UPnP)

L'utilisateur a clarifié l'architecture attendue :

1. **Media server** : implémente correctement l'action UPnP `Search` — retourne un DIDL contenant des containers virtuels navigables (les 4 groupes). Les IDs de ces containers (`qobuz:search:catalog:artists:camille`, etc.) sont opaques pour le control point.

2. **GetSearchCapabilities** : doit retourner des caps non vides pour que les control points (BubbleUPnP, PMOMusic frontend) reconnaissent le serveur comme searchable. **BubbleUPnP ne reconnaissait pas PMOMusic comme searchable avant les modifications récentes.**

3. **Control point / Frontend** : envoie `Search(ContainerID, SearchCriteria)` → reçoit DIDL avec des containers → les navigue via Browse normalement. Le control point ne connaît RIEN des IDs internes Qobuz.

4. **Pas de endpoint `/search` spécifique Qobuz** dans le control point — c'est l'action UPnP standard `Search` qui fait tout.

## Fix correct à implémenter

### Côté frontend (`MediaBrowser.vue` + `useMediaServers.ts`)

Supprimer `isSearchMode`, `searchResults`, `searchQuery`. Remplacer par :

```typescript
// browseData devient simplement :
const browseData = computed(() =>
    getBrowseCached(props.serverId, props.containerId)
);
```

`searchServer()` doit stocker dans `browseCache` sous l'ID retourné par le serveur et naviguer vers cet ID. Quand l'utilisateur clique ensuite sur un sous-container (Artistes, Albums…), `browseContainer` est appelé avec l'ID correct, le serveur retourne les bons résultats, le cache est peuplé, `browseData` l'affiche.

La clé : **sortir du search mode dès que la navigation a eu lieu**. Ce que `isSearchMode` empêche actuellement.

### Côté serveur (`pmocontrol/src/pmoserver_ext.rs`)

L'endpoint REST `/servers/{id}/search` appelle `server.search("0", query, 0, 200)` via UPnP Search. Il retourne actuellement `container_id: "search"` (fictif). 

Il devrait retourner le vrai `container_id` issu du DIDL (ex: `qobuz:search:catalog:all:camille`) pour que le frontend puisse le mettre dans le cache et naviguer vers un ID que le serveur reconnaît lors d'un Browse ultérieur.

**Mais** : mettre la logique de construction de cet ID dans le control point viole la séparation des couches. La bonne approche est que le serveur retourne dans le DIDL des containers avec des IDs navigables, et que le control point les utilise tels quels.

### Vérifier aussi

- `GetSearchCapabilities` dans `pmomediaserver/src/content_handler.rs` retourne `"dc:title,dc:creator,upnp:artist,upnp:album,upnp:genre"` — vérifier que c'est bien annoncé dans le service descriptor UPnP (sinon BubbleUPnP ne propose pas la recherche).

## Fichiers clés

| Fichier | Rôle |
|---|---|
| `pmoapp/webapp/src/components/pmocontrol/MediaBrowser.vue` | Bug `isSearchMode` / `browseData` |
| `pmoapp/webapp/src/composables/useMediaServers.ts` | `searchServer()`, `browseCache` |
| `pmoapp/webapp/src/services/pmocontrol/api.ts` | Appel REST `/search` |
| `pmocontrol/src/pmoserver_ext.rs` | Handler REST `search_server()` |
| `pmomediaserver/src/contentdirectory/handlers.rs` | UPnP `search_handler()`, logs `━━━ SEARCH ━━━` |
| `pmomediaserver/src/content_handler.rs` | `ContentHandler::search()` |
| `pmoqobuz/src/source.rs` | `search_grouped()`, `execute_search()`, `parse_object_id()` |

## Format des IDs virtuels Qobuz

```
qobuz:search:{scope}:{type}:{query}
  scope : catalog | favorites
  type  : all | albums | artists | tracks | playlists
  query : texte libre (peut contenir ':' — splitn(5) utilisé)
```

Exemples :
- `qobuz:search:catalog:all:camille` → Browse → 4 containers groupés
- `qobuz:search:catalog:artists:camille` → Browse → liste d'artistes
- `qobuz:search:favorites:albums:bach` → Browse → albums favoris

## État du code à la fin de la session

Les logs de debug (`━━━ BROWSE ━━━`, `━━━ SEARCH ━━━`, preview DIDL) ont été ajoutés dans `handlers.rs` au niveau `warn`. Le Makefile a été fixé pour propager `RUST_LOG` à travers `osascript`. La logique serveur Qobuz fonctionne. Seul le frontend est cassé.
