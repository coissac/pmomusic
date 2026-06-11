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

## 4. Pagination concurrente des playlists — **À FAIRE** (priorité moyenne)

**Problème** : `pmoqobuz` charge les pages de tracks d'une playlist séquentiellement (offset=0, puis
offset=500, etc.). Chaque requête attend la précédente.

**Ce que fait qbz** (`get_playlist`, l.1397) :
- Page 1 → récupère les métadonnées + `total` track count
- Pages 2..N → lancées **concurremment** via `join_all` dès que `total` est connu
- Résultats ré-ordonnés par offset avant fusion

**Impact pour pmoqobuz** : une playlist de 2 000 tracks (4 pages de 500) passe de 4 requêtes
séquentielles (~1,6 s) à 1 + 3 en parallèle (~0,7 s).

**Note** : à implémenter avec un semaphore (comme le CMAF) pour ne pas surcharger l'API Qobuz.

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

## Résumé de priorités

| # | Amélioration | Effort | Impact | État |
|---|---|---|---|---|
| 1 | Streaming CMAF | Élevé | Critique (pipeline futur) | **Fait** |
| 2 | Bundle extraction avec cache disque | Moyen | Élevé (résilience) | **Fait** |
| 3 | Batch `track/getList` | Faible | Élevé (performances) | **Fait** |
| 4 | Pagination concurrente playlists | Faible | Moyen | À faire |
| 5 | Release watch endpoint | Faible | Faible (catalogue) | À faire |
| 6 | `extra=track_ids` + batch à deux passes | Faible | Faible (optimisation) | À faire |
