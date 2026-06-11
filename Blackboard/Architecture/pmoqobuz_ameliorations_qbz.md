# Améliorations pmoqobuz inspirées de qbz

Analyse comparative avec le projet [qbz](../../../qbz) (`crates/qbz-qobuz/`), client Qobuz Rust plus avancé.
Les items sont classés par priorité et état d'avancement.

---

## 1. Streaming CMAF — **FAIT**

**Problème** : l'endpoint legacy `/track/getFileUrl` est en cours de dépréciation. Qobuz bascule vers
CMAF (Common Media Application Format) : segments AES-CTR chiffrés sur CDN Akamai.

**Implémentation réalisée** :
- `pmoqobuz::cmaf` — pipeline complet : dérivation HKDF, dérobage AES-CBC, déchiffrement AES-CTR par frame
- `pmoqobuz::retry` — retry exponentiel avec classification Transient/Terminal
- `QobuzClient::open_cmaf_stream()` — `AsyncRead` progressif via `tokio::io::duplex`, 3 segments en vol
- `GET /qobuz/tracks/:id/flac` — endpoint REST local qui expose le flux ; `LazyProvider::get_url()`
  retourne cette URL locale (via `PMO_SERVER_URL`) → le progressive caching de pmocache est préservé sans
  modification

**Référence qbz** : `crates/qbz-qobuz/src/cmaf.rs`

---

## 2. Extraction du bundle Qobuz — **Fait**

**Problème** : `pmoqobuz` utilise un `app_id` et un `configvalue` statiques, hardcodés ou configurés
manuellement. Qobuz peut les invalider à tout moment en changeant son bundle JS.

**Ce que fait qbz** (`bundle.rs`) :
- Télécharge la page `https://play.qobuz.com/login`, extrait l'URL du bundle JS
- Parse le bundle (~7 MB) avec des regex pour en extraire `app_id`, les secrets, et la `private_key` OAuth
- Met en cache les tokens sur disque avec un hash de version du bundle (`bundle_version`)
- Revalide automatiquement si la version change (rotation silencieuse de Qobuz)
- Timeout de 45 s sur le fetch, 2 retry sur extraction

**Impact pour pmoqobuz** :
- Le `Spoofer` actuel fait un fetch similaire mais sans cache disque ni détection de version
- Ajouter `CachedBundle` (version + tokens + timestamp) dans `pmoconfig` ou dans le répertoire de données
- Relire le cache au démarrage, re-extraire seulement si la version du bundle a changé

**Avantage** : ne jamais tomber en panne quand Qobuz rotate ses secrets sans préavis.

---

## 3. Chargement batch de tracks — `track/getList` — **FAIT**

**Problème** : les tracks retournées par `/playlist/get?extra=tracks` et
`/favorite/getUserFavorites` ont des métadonnées incomplètes (parfois sans `performer`,
jamais de `sample_rate`/`bit_depth`/`channels`). Cela déclenchait des appels individuels
`get_track` lazy à la lecture.

**Implémentation réalisée** :
- `signing::sign_track_get_list(ids_csv, timestamp, secret)` — signature MD5 pour `track/getList`
- `QobuzApi::post_json_with_query` — POST avec auth headers + query sig + JSON body
- `QobuzApi::get_tracks_batch(&[&str])` — fenêtres de 50 IDs, appels en série
- `QobuzClient::get_tracks_batch` — wrapper avec cache (skip les IDs déjà en cache)
- `get_playlist_tracks` : phase 1 pagination existante, phase 2 enrichissement si secret disponible
- `get_favorite_tracks` : même enrichissement en phase 2
- Fallback gracieux si le secret est absent ou si `track/getList` échoue

**Ce que fait qbz** (`get_tracks_batch`, l.1323) :
```
POST /track/getList
{ "tracks_id": [id1, id2, ..., id50] }
→ { "tracks": { "total": N, "items": [...Track] } }
```
- Fenêtre de 50 IDs max par appel (limite API Qobuz)
- Les fenêtres supérieures à 50 sont découpées et appelées en série (respecte les quotas)

---

## 4. Pagination concurrente des playlists — **FAIT**

**Implémentation réalisée** dans `QobuzApi::get_playlist_tracks` :
- Page size augmentée de 50 → **500** (réduit le nombre de pages de 10×)
- Page 1 séquentielle pour obtenir `total`
- Pages 2..N lancées en parallèle via `futures::try_join_all` + `Semaphore(3)`
- Résultats triés par offset avant fusion — ordre playlist garanti
- Suivi de phase 2 (`track/getList`) inchangé

**Impact** : playlist de 2 000 tracks (4 pages de 500) → 1 séquentielle + 3 parallèles ≈ 0,7 s
au lieu de 4 séquentielles ≈ 1,6 s. Playlists ≤ 500 tracks : 1 seule requête.

---

## 5. Endpoint `release_watch` — **À FAIRE** (priorité basse)

**Problème** : pmoqobuz ne supporte pas les nouvelles sorties d'artistes suivis.

**Ce que fait qbz** (`get_release_watch`, l.809) :
```
GET /favorite/getNewReleases?type=album&limit=50&offset=0
→ { has_more: bool, items: [...Album] }
```
- Types disponibles : `album`, `live`, `ep_single`
- Pas de champ `total` dans la réponse — pagination via `has_more`

**Impact pour pmoqobuz** : permettre un "quoi de neuf" dans l'interface — albums des artistes favoris
sortis récemment. Utile pour le catalogue de la webapp.

---

## 6. Chargement `playlist/get?extra=track_ids` — **À FAIRE** (priorité basse)

**Ce que fait qbz** (`get_playlist_track_ids`, l.1296) :
- Variante légère de `playlist/get` qui retourne uniquement les IDs (pas les objets Track complets)
- Utile pour vérifier si une playlist a changé sans tout recharger
- Combiné avec `get_tracks_batch` pour un chargement optimal en deux passes :
  1. `playlist/get?extra=track_ids` → liste d'IDs
  2. `track/getList` par fenêtres de 50 → objets Track complets

---

---

## 7. Robustesse des requêtes et du parsing API

Analyse comparative approfondie (`qbz/crates/qbz-qobuz/src/`) révélant quatre gaps dans
pmoqobuz par rapport à qbz.

---

### 7a. Signature générique — **À FAIRE** (priorité basse, effort très faible)

**Problème** : pmoqobuz a une fonction de signature dédiée par endpoint
(`sign_track_get_file_url`, `sign_userlib_get_albums`, `sign_track_get_list`). Chaque nouvel
endpoint signé nécessite une nouvelle fonction, avec risque de divergence silencieuse.

**Ce que fait qbz** (`auth.rs`, l.55-60) :
```rust
fn sign_request(method_name: &str, params: &[(&str, &str)], timestamp: u64, secret: &str) -> String {
    // Concatène method + pairs key+value triées alphabétiquement + timestamp + secret
    // MD5 du résultat
}
```
Tous les endpoints partagent la même logique. Ajouter un endpoint = zéro code de signature.

**Pour pmoqobuz** : remplacer les 3 fonctions par une `sign_request` générique.
Le tri alphabétique des paramètres est implicitement respecté par nos fonctions actuelles
(vérifier que l'ordre de `sign_track_get_list` correspond bien à la convention qbz).

---

### 7b. Métadonnées audio dans `TrackResponse` — **À FAIRE** (priorité haute, effort faible)

**Problème** : `TrackResponse` (la struct de désérialisation interne) ne capte pas les champs
de qualité audio retournés par `track/get` et `track/getList` :

```
maximum_sampling_rate  → absente de TrackResponse
maximum_bit_depth      → absente de TrackResponse
hires_streamable       → absente de TrackResponse
```

Conséquence : après notre `get_tracks_batch`, les champs `Track.sample_rate` et
`Track.bit_depth` restent `None` (ils sont `#[serde(skip)]` dans `models.rs`), alors que
l'API les a retournés. La qualité audio n'est connue qu'après lecture effective via CMAF.

**Ce que fait qbz** (`types.rs`, l.204-215) :
```rust
pub struct Track {
    pub maximum_sampling_rate: Option<f64>,  // 44100.0, 96000.0, 192000.0
    pub maximum_bit_depth: Option<u32>,       // 16, 24
    pub hires_streamable: bool,
    ...
}
```

**Pour pmoqobuz** :
1. Ajouter `maximum_sampling_rate: Option<f64>`, `maximum_bit_depth: Option<u32>` à `TrackResponse`
2. Les propager dans `Track` via `parse_track` (remplacer les `#[serde(skip)]`)
3. Ces valeurs alimentent `AudioMetadata` dans `register_tracks_lazy` sans attendre la lecture

**Impact** : les métadonnées hi-res (24-bit/96kHz) sont disponibles dès le chargement de la
playlist, pas seulement après la première lecture.

---

### 7c. Parsing des restrictions de stream — **À FAIRE** (priorité moyenne, effort moyen)

**Problème** : la réponse de `track/getFileUrl` contient un champ `restrictions[]` qui signale
des blocages (ex: `"FormatRestrictedByFormatAvailability"`, `"SampleRestrictedByRightHolders"`).
pmoqobuz ne le parse pas — un track restreint retourne une URL qui échoue silencieusement à
la lecture.

**Ce que fait qbz** (`types.rs`, l.92-112, `client.rs`, l.1959-2012) :
```rust
pub struct StreamUrl {
    pub url: String,
    pub restrictions: Vec<StreamRestriction>,
    ...
}

pub fn has_restrictions(&self) -> bool {
    self.restrictions.iter().any(|r| {
        r.code == "FormatRestrictedByFormatAvailability"
            || r.code == "SampleRestrictedByRightHolders"
    })
}
```
Si `has_restrictions()`, qbz essaie la qualité inférieure suivante (voir 7d).

**Pour pmoqobuz** :
- Ajouter `restrictions: Vec<StreamRestriction>` au parsing de `FileUrlResponse` dans `catalog.rs`
- Retourner une erreur explicite (`QobuzError::TrackRestricted`) si restrictions présentes
- Prépare la base pour le fallback de qualité (7d)

---

### 7d. Fallback automatique de qualité — **À FAIRE** (priorité moyenne, effort moyen)

**Problème** : si le format demandé (ex: Hi-Res 24-bit) n'est pas disponible pour un track,
`get_file_url` échoue. pmoqobuz n'a pas de dégradation automatique.

**Ce que fait qbz** (`client.rs`, l.1959-2012) :
```
UltraHiRes (27) → HiRes (7) → Lossless (6) → MP3 (5)
```
Essaie chaque qualité jusqu'à obtenir une URL sans restrictions. Retourne
`TrackUnavailable` seulement si toutes les qualités échouent.

**Pour pmoqobuz** : ajouter `get_file_url_with_fallback` dans `catalog.rs` qui itère sur
`[format_id_configured, 6 (lossless), 5 (mp3)]` jusqu'à succès.
Le path CMAF n'est pas concerné (format géré côté serveur).

---

### 7e. Respect du header `Retry-After` sur 429 — **À FAIRE** (priorité moyenne, effort moyen)

**Problème** : `retry.rs` classifie correctement les 429 comme transitoires, mais le backoff
est fixe (250 ms → 500 ms → 1 s). Qobuz peut indiquer un délai précis via le header
`Retry-After`. L'ignorer risque soit de retentar trop tôt (nouveau 429), soit d'attendre trop
longtemps (backoff fixe parfois plus long que nécessaire).

**Ce que fait qbz** (`client.rs`, l.2497-2505) :
```rust
if status == StatusCode::TOO_MANY_REQUESTS {
    let retry_after = response.headers()
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(2);
    return Err(ApiError::RateLimited(retry_after));
}
```
Le délai est passé à la logique de retry qui dort exactement `retry_after` secondes.

**Pour pmoqobuz** : dans `mod.rs::handle_response`, sur 429, lire le header et
propager la valeur via une variante `QobuzError::RateLimited(u64)`.
`call_with_auth_repair` dans `client.rs` peut ensuite `tokio::time::sleep` ce délai
avant de retenter, au lieu du backoff fixe.

---

---

## 8. Recherche — **À FAIRE** (priorité haute)

pmoqobuz n'expose aucune recherche. qbz montre que Qobuz a un vrai moteur de recherche
multi-catégories.

### 8a. Endpoints disponibles

```
GET /album/search?query=…&limit=…&offset=…[&type=…]
GET /track/search?query=…&limit=…&offset=…[&type=…]
GET /artist/search?query=…&limit=…&offset=…[&type=…]
GET /playlist/search?query=…&limit=…&offset=…
GET /catalog/search?query=…&limit=…&offset=…     ← combiné (albums + tracks + artists + playlists)
```

Tous non-authentifiés (pas de token requis). Signature pattern :
`sign_search(method, query, limit, offset, search_type, timestamp, secret)`
où les params signés sont concaténés **dans l'ordre alphabétique** :
`limit{L}offset{O}query{Q}[type{T}]`.

### 8b. Paramètre `type` (filtre sémantique)

Sur album/track/artist search, le param `type` affine la signification de `query` :
- `MainArtist` — cherche dans le nom de l'artiste principal
- `Performer` — cherche dans les performers/interprètes
- `Composer` — cherche dans le compositeur
- `Label` — cherche dans le nom du label
- `ReleaseName` — cherche dans le titre de la release

Sans `type`, la recherche est full-text sur tous les champs.

### 8c. Structure de retour

```rust
pub struct SearchResultsPage<T> {
    pub items: Vec<T>,
    pub total: u32,
    pub offset: u32,
    pub limit: u32,
}
```

`catalog/search` retourne un objet avec clés `albums`, `tracks`, `artists`, `playlists`,
`most_popular` — chacun étant une `SearchResultsPage<T>` — mais qbz le désérialise en `Value`
brut (pas de struct dédiée).

### 8d. Ce qu'il faut implémenter dans pmoqobuz

1. `signing::sign_search(method, query, limit, offset, search_type, ts, secret) -> String`
   (signature spécifique avec ordre alpha des params)
2. `QobuzApi::search_tracks(query, limit, offset, search_type) -> Result<SearchPage<Track>>`
3. `QobuzApi::search_albums(query, limit, offset, search_type) -> Result<SearchPage<Album>>`
4. `QobuzApi::search_artists(query, limit, offset) -> Result<SearchPage<Artist>>`
5. `QobuzApi::catalog_search(query, limit, offset) -> Result<CatalogSearchResult>`
   avec `CatalogSearchResult { albums, tracks, artists, playlists }`
6. Exposer via `QobuzClient` + endpoint REST `/qobuz/search?q=…&type=track|album|artist|all`

Priorité : **catalog_search** en premier (un seul endpoint couvre tous les cas UI).

---

## 9. Découverte (Discover) et playlists éditoriales — **À FAIRE** (priorité moyenne)

Les "Daily Q", "Weekly Q" et radios ne sont **pas** des endpoints API dynamiques distincts.
Ce sont des playlists Qobuz standard (avec des IDs fixes par compte), accessibles via
`/playlist/get`. Ce qui manque, c'est l'accès au catalogue de découverte éditorialisé.

### 9a. Endpoints Discover

```
GET /discover/index?[genre_ids=112,119]              ← tableau de bord
GET /discover/playlists?[tags=…&genre_ids=…]&limit=…&offset=…
GET /discover/newReleases?[genre_ids=…]&limit=…&offset=…
GET /discover/mostStreamed?[genre_ids=…]&limit=…&offset=…
GET /discover/albumOfTheWeek?[genre_ids=…]
GET /discover/pressAward?[genre_ids=…]&limit=…&offset=…
GET /discover/qobuzissims?[genre_ids=…]&limit=…&offset=…
GET /discover/idealDiscography?[genre_ids=…]&limit=…&offset=…
```

Tous authentifiés. Signature : `sign_request("discover{endpoint_slug}", params, ts, secret)`.

### 9b. Tags de playlists

```
GET /playlist/getTags
→ Vec<PlaylistTag { id, slug, name (localisé) }>
```

Permet de filtrer `discover/playlists` par tag (`partner`, `label`, etc.).

### 9c. Albums mis en avant

```
GET /album/getFeatured?type={new-releases|press-awards|most-streamed}[&genre_id=…]
→ SearchResultsPage<Album>
```

Alternative à `discover/newReleases` qui retourne des albums complets avec métadonnées.

### 9d. Structure `DiscoverResponse`

```rust
pub struct DiscoverResponse {
    pub containers: DiscoverContainers,
}
pub struct DiscoverContainers {
    pub playlists: Option<DiscoverContainer<DiscoverPlaylist>>,
    pub new_releases: Option<DiscoverContainer<DiscoverAlbum>>,
    pub most_streamed: Option<DiscoverContainer<DiscoverAlbum>>,
    pub qobuzissims: Option<DiscoverContainer<DiscoverAlbum>>,
    pub album_of_the_week: Option<DiscoverContainer<DiscoverAlbum>>,
    pub press_awards: Option<DiscoverContainer<DiscoverAlbum>>,
    pub ideal_discography: Option<DiscoverContainer<DiscoverAlbum>>,
    pub playlists_tags: Option<DiscoverContainer<PlaylistTag>>,
}
```

### 9e. Daily Q / Weekly Q / Radio

Ces playlists sont des **playlists Qobuz standard** générées par Qobuz dans la bibliothèque
utilisateur. Elles apparaissent dans `getUserPlaylists` avec des noms spéciaux. Il n'y a pas
d'endpoint dédié — elles se chargent comme n'importe quelle playlist via `/playlist/get`.

Pour les exposer, il suffit de :
1. Ajouter un filtre dans `get_user_playlists` pour identifier ces playlists (par propriétaire
   `qobuz` + nom pattern) et les exposer séparément dans l'API REST
2. Ou laisser l'UI trier les playlists par propriétaire

---

## Résumé de priorités

| # | Amélioration | Effort | Impact | État |
|---|---|---|---|---|
| 1 | Streaming CMAF | Élevé | Critique (pipeline futur) | **Fait** |
| 2 | Bundle extraction avec cache disque | Moyen | Élevé (résilience) | **Fait** |
| 3 | Batch `track/getList` | Faible | Élevé (performances) | **Fait** |
| 4 | Pagination concurrente playlists | Faible | Moyen | **Fait** |
| 5 | Release watch endpoint | Faible | Faible (catalogue) | À faire |
| 6 | `extra=track_ids` + batch à deux passes | Faible | Faible (optimisation) | À faire |
| 7a | Signature générique `sign_request` | Très faible | Maintenabilité | À faire |
| 7b | Métadonnées audio dans TrackResponse | Faible | Élevé (qualité metadata) | À faire |
| 7c | Parsing restrictions stream | Moyen | Moyen (robustesse) | À faire |
| 7d | Fallback automatique de qualité | Moyen | Moyen (robustesse) | À faire |
| 7e | Respect `Retry-After` 429 | Moyen | Moyen (résilience rate limit) | À faire |
| 8 | Recherche (track/album/artist/catalog) | Moyen | Élevé (fonctionnalité manquante) | À faire |
| 9 | Discover + playlists éditoriales | Moyen | Moyen (catalogue) | À faire |
