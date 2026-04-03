** Ce travail devra être réalisé en suivant scrupuleusement les consignes listées dans le fichier [@Rules_optimal.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules_optimal.md) **

## Problème

Quand le site est accédé via une adresse externe (reverse proxy, ex: `orbis.petite-maison-orange.fr`),
les URLs d'images et de ressources générées par le backend contiennent l'IP locale hardcodée
(ex: `http://192.168.0.32:80/covers/image/...`). Ces URLs sont inaccessibles depuis l'extérieur.

## Cause racine

Il existe deux contextes distincts de construction d'URL dans pmomusic :

**A. Contexte UPnP / réseau local** : les URLs doivent être absolues avec l'IP locale, car les
renderers UPnP accèdent directement aux ressources sur le réseau local.
→ Elles utilisent correctement `PMO_SERVER_URL` / `covers_absolute_url_for()`.

**B. Contexte HTTP / frontend webapp** : les URLs retournées au navigateur doivent refléter l'hôte
vu par le client (local ou via proxy). Elles utilisent actuellement aussi `PMO_SERVER_URL` —
c'est le bug.

`pmoserver` dispose déjà de `request_base_url(headers)` (`pmoserver/src/server.rs:633`) qui lit
`X-Forwarded-Proto` / `X-Forwarded-Host` / `Host` et retourne la base URL correcte par requête.
Mais **aucun handler HTTP ne l'utilise** pour construire les URLs retournées au frontend.

## Solution : Middleware Axum BaseUrl

Ajouter un middleware Axum au niveau de la racine du routeur qui enrichit chaque requête d'une
`Extension<BaseUrl>` calculée depuis les headers. Tous les handlers HTTP qui retournent des URLs
au frontend extraient cette extension — **un seul point de calcul, zéro gestion ad hoc**.

### 1. Nouveau type `BaseUrl` dans `pmoserver/src/lib.rs`

```rust
/// URL de base effective pour la requête courante.
/// Calculée depuis X-Forwarded-Proto/Host ou Host header.
/// Injectée par `base_url_layer` dans toutes les requêtes Axum.
#[derive(Debug, Clone)]
pub struct BaseUrl(pub String);
```

### 2. Middleware `base_url_layer` dans `pmoserver/src/lib.rs`

```rust
/// Middleware Axum : injecte BaseUrl dans chaque requête.
/// À appliquer sur le routeur racine via `.layer(base_url_layer())`.
pub fn base_url_layer() -> axum::middleware::FromFnLayer<...> {
    axum::middleware::from_fn(|request: Request, next: Next| async move {
        // Fallback sur PMO_SERVER_URL (valeur de démarrage avec la vraie IP/port),
        // pas sur localhost:8080 hardcodé.
        let base = get_request_base_url(request.headers())
            .or_else(|| std::env::var("PMO_SERVER_URL").ok())
            .unwrap_or_else(|| {
                tracing::warn!("BaseUrl: aucun header Host/X-Forwarded-Host ni PMO_SERVER_URL — fallback localhost:8080");
                "http://localhost:8080".to_string()
            });
        let mut request = request;
        tracing::debug!("BaseUrl calculée : {}", base);
        request.extensions_mut().insert(BaseUrl(base));
        next.run(request).await
    })
}
```

### 3. Application du layer dans `pmoserver/src/server.rs`

Dans Axum, le dernier `.layer()` appliqué est le plus extérieur (exécuté en premier sur la
requête entrante). Pour que `base_url_layer` voie les headers **après** tout layer de nettoyage,
il doit être **intérieur** — donc appliqué **avant** dans le code :

```rust
router
    .layer(header_clean_layer())  // extérieur → exécuté en premier, nettoie les headers
    .layer(base_url_layer())      // intérieur → voit les headers nettoyés
```

Les endpoints UPnP (SSDP, description XML, control, event) ne doivent pas appeler
`covers_absolute_url_for_upnp()` via `BaseUrl` — l'injection du middleware ne les affecte pas
puisqu'ils n'extraient pas `Extension<BaseUrl>`.

Si des routes non-HTTP sont ajoutées ultérieurement (métriques internes, health checks sans
contexte client), les isoler dans un sous-routeur dédié sans `base_url_layer()`.

### 4. Utilisation dans les handlers

Tous les handlers qui retournent des URLs au frontend ajoutent :

```rust
Extension(base_url): Extension<BaseUrl>,
```

Et utilisent `base_url.url_for(&pmocovers::covers_route_for(pk, None))` à la place de
`covers_absolute_url_for()` (voir section 5 pour le pattern complet).

Handlers REST concernés (liste non exhaustive) :
- `pmocontrol/src/pmoserver_ext.rs` : `get_renderer_full_snapshot` (album_art_uri dans snapshot)
- `pmocontrol/src/pmoserver_ext.rs` : handler browse (ContainerEntry.album_art_uri)
- `pmoradiofrance/src/api_rest.rs` : endpoints playlist/metadata
- `pmoplaylist/src/handle/read.rs` : album art dans les réponses playlist

**Handlers SSE** (`pmocontrol/src/sse.rs`) : cas particulier. Le stream SSE est long-lived —
après le `stream!` block, on n'est plus dans le contexte du handler Axum. `BaseUrl` doit être
clonée dans une variable locale **avant** le `stream!`, puis `move`-ée dans la closure :

```rust
pub async fn renderer_events_sse(
    State(control_point): State<Arc<ControlPoint>>,
    Extension(base_url): Extension<BaseUrl>,   // ← extraite à la connexion
) -> impl IntoResponse {
    let base_url = base_url.clone(); // clone avant le stream! pour le move
    // ...
    let stream = stream! {
        while let Some(event) = rx_tokio.recv().await {
            // base_url est disponible ici par move
            let payload = renderer_event_to_payload(event, &base_url);
            yield Ok(Event::default()...);
        }
    };
}

### 5. Méthode `url_for` sur `BaseUrl` + fonctions `route_for` dans chaque crate

La combinaison `base_url + route` est identique pour tous les types de ressources. Elle est
factorisée en une méthode sur `BaseUrl` dans `pmoserver/src/lib.rs` :

```rust
impl BaseUrl {
    /// Construit une URL absolue en combinant la base URL de la requête avec une route relative.
    /// Usage : base_url.url_for(&pmocovers::covers_route_for(pk, None))
    pub fn url_for(&self, route: &str) -> String {
        debug_assert!(route.starts_with('/'), "route must start with '/'");
        format!("{}{}", self.0.trim_end_matches('/'), route)
    }
}
```

Chaque crate spécialisée expose uniquement sa **route** (chemin relatif), pas l'URL complète :

**`pmocovers/src/lib.rs`** — déplacer depuis `pmocache` :
```rust
/// Route relative d'une cover : `/covers/image/{pk}[/{param}]`
pub fn covers_route_for(pk: &str, param: Option<&str>) -> String { ... }
```

**`pmoaudiocache/src/lib.rs`** :
```rust
/// Route relative d'un fichier audio : `/audio/flac/{pk}`
pub fn audio_route_for(pk: &str) -> String {
    format!("/audio/flac/{}", pk)
}
```

Usage dans les handlers :
```rust
base_url.url_for(&pmocovers::covers_route_for(pk, None))
base_url.url_for(&pmoaudiocache::audio_route_for(pk))
```

### 6. Renommage de `covers_absolute_url_for` → `covers_absolute_url_for_upnp`

Pour rendre le contexte d'usage explicite et décourager l'appel depuis les handlers HTTP,
renommer dans `pmocache/src/lib.rs` :

```rust
// Ancien nom — marqué deprecated pour faciliter la migration (warnings à la compilation)
#[deprecated(note = "Utiliser covers_absolute_url_for_upnp() dans les contextes UPnP uniquement")]
pub fn covers_absolute_url_for(pk: &str, param: Option<&str>) -> String { ... }

// Nouveau nom — usage UPnP uniquement
pub fn covers_absolute_url_for_upnp(pk: &str, param: Option<&str>) -> String { ... }
```

Mettre à jour tous les appels existants (contextes UPnP/DIDL uniquement) via un grep :
`grep -rn "covers_absolute_url_for" src/ --include="*.rs"`

À terme, `covers_route_for` et `covers_absolute_url_for_upnp` devraient migrer de `pmocache`
vers `pmocovers`, mais ce n'est pas le périmètre de ce ticket.

## URLs dans les documents DIDL et SSE

Les documents DIDL bruts (`<res>`, `<upnp:albumArtURI>`) ne transitent jamais vers le frontend —
pmocontrol les parse côté serveur et n'envoie que des champs extraits (JSON) via REST et SSE.
Il n'y a donc pas de "rebasage XML" : les champs extraits (`album_art_uri`, etc.) passent tous
par des handlers qui ont accès à `BaseUrl`.

**SSE est per-client** : chaque connexion SSE crée son propre receiver (`subscribe_events()`).
La `BaseUrl` est figée à l'établissement de la connexion et ne sera pas mise à jour si le client
change de réseau en cours de stream — comportement attendu et documenté.
Il n'y a ni canal partagé, ni duplication LAN/WAN. Le handler SSE capture `Extension<BaseUrl>`
à l'établissement de la connexion et applique `base_url.url_for()` à toutes les URLs des événements
émis vers ce client.

Les DIDL servis directement aux renderers UPnP (hors HTTP webapp) gardent l'IP locale — c'est
correct, les renderers sont sur le réseau local.

## Audit préalable à l'implémentation

Avant de modifier les handlers, faire un audit exhaustif de tous les endroits qui construisent
des URLs absolues dans des réponses JSON au frontend :

```bash
# Appels directs aux fonctions URL connues
grep -rn "covers_absolute_url_for\|audio/flac\|cache/audio" src/ --include="*.rs"

# Constructions format! utilisant PMO_SERVER_URL ou des littéraux http://
grep -rn "PMO_SERVER_URL\|format!.*base_url\|format!.*server_url" src/ --include="*.rs"
grep -rn 'format!.*"http' src/ --include="*.rs"
```

Note : les PKs de covers et audio sont des hashes hex (`[0-9a-f]+`) — ils ne peuvent pas
contenir de caractères spéciaux nécessitant un encodage URL. La concaténation `format!` est
donc sûre ; pas besoin de `url::Url::join`.

## Tests à écrire

- **Middleware** : `BaseUrl` correctement extraite depuis `X-Forwarded-Host`, `Host`, et en
  leur absence (fallback sur `PMO_SERVER_URL`)
- **`url_for`** : assertion que toutes les routes commencent par `/` ; pas de double slash ;
  trailing slash sur la base géré par `trim_end_matches`
- **Handlers REST** : `album_art_uri` rebased dans `FullRendererSnapshot` et `BrowseResponse`
- **SSE** : URLs rebased dans les événements `TrackChanged`
- **UPnP** : vérifier que les URLs servies aux renderers UPnP restent en IP locale (non affectées
  par `BaseUrl`)
- **Intégration** : appeler **chaque endpoint frontend** avec un client HTTP de test pour
  vérifier (a) qu'aucune panique ne se produit (middleware bien appliqué) et (b) que les URLs
  produites utilisent l'hôte du header `X-Forwarded-Host` simulé et non l'IP locale.
  La panique sur `Extension<BaseUrl>` manquante est un comportement voulu — elle doit être
  détectée par ces tests et non silencieusement masquée par un `Option`.

## Sécurité : headers X-Forwarded-*

Les headers `X-Forwarded-Proto` / `X-Forwarded-Host` peuvent être forgés par n'importe quel client
si le reverse proxy ne les filtre pas. Dans le contexte de déploiement de pmomusic (usage domestique,
proxy Nginx/Caddy unique), le risque est faible et hors périmètre de ce ticket.

À surveiller si le déploiement évolue : restreindre la lecture de ces headers aux requêtes venant
de l'IP du proxy (liste blanche de proxies de confiance côté Axum ou côté proxy).

## Périmètre : ce qui ne change PAS

- `covers_absolute_url_for()` dans `pmocache` : conservée pour les contextes UPnP
- `PMO_SERVER_URL` env var : conservée pour UPnP et les processus non-HTTP
- URLs dans les DIDL servis aux renderers UPnP : inchangées (doivent rester en IP locale)
- `server_base_url` passé aux sources (RadioFrance, RadioParadise, Qobuz) : inchangé
  (ces sources construisent des URLs pour les renderers réseau)

## Plan d'exécution

### Corrections d'audit préalables

Divergences entre le document et le code réel :

- **Route audio** : `/audio/tracks/{pk}` (PAS `/audio/flac/{pk}`)
- **`covers_route_for`** existe déjà dans `pmocache/src/lib.rs:149` — à copier vers `pmocovers`
- **`album_art_uri`** dans les handlers est propagé depuis des caches amont ; le point de
  construction réel est `pmoradiofrance/src/metadata_cache.rs:263` (tâche de fond, pas un handler)
- **`pmoqobuz/src/source.rs:1943`** construit des URLs audio avec `self.base_url` → contexte
  UPnP/renderer, hors périmètre de ce ticket

### Étape 0 — Audit exhaustif (avant tout changement)

```bash
grep -rn "covers_absolute_url_for\|audio/tracks\|cache/audio" --include="*.rs"
grep -rn "PMO_SERVER_URL\|format!.*base_url\|format!.*server_url" --include="*.rs"
grep -rn 'format!.*"http' --include="*.rs"
```

Identifier tous les call sites dans les contextes HTTP (handlers, caches de métadonnées servant
le frontend). Distinguer des contextes UPnP/renderer (hors périmètre).

### Étape 1 — `pmoserver/src/lib.rs` : ajouter `BaseUrl` + `base_url_layer`

`get_request_base_url(headers)` existe déjà à la ligne 199. Ajouter :

```rust
use axum::{extract::Request, middleware::Next, response::Response};

#[derive(Debug, Clone)]
pub struct BaseUrl(pub String);

impl BaseUrl {
    pub fn url_for(&self, route: &str) -> String {
        debug_assert!(route.starts_with('/'), "route must start with '/'");
        format!("{}{}", self.0.trim_end_matches('/'), route)
    }
}

pub async fn base_url_middleware(mut request: Request, next: Next) -> Response {
    let base = get_request_base_url(request.headers())
        .unwrap_or_else(|| {
            std::env::var("PMO_SERVER_URL").unwrap_or_else(|_| {
                tracing::warn!(
                    "BaseUrl: aucun header Host/X-Forwarded-Host ni PMO_SERVER_URL \
                     — fallback localhost:8080"
                );
                "http://localhost:8080".to_string()
            })
        });
    tracing::debug!("BaseUrl calculée : {}", base);
    request.extensions_mut().insert(BaseUrl(base));
    next.run(request).await
}

pub fn base_url_layer() -> axum::middleware::FromFnLayer<...> {
    axum::middleware::from_fn(base_url_middleware)
}
```

### Étape 2 — `pmoserver/src/server.rs` : appliquer le layer

Trouver la construction du routeur principal. Ajouter `base_url_layer()` avant les layers
existants (= intérieur dans la pile Tower) :

```rust
router
    .layer(some_existing_layer())   // extérieur → exécuté en premier
    .layer(base_url_layer())        // intérieur → voit les headers après nettoyage
```

### Étape 3 — `pmocovers/src/lib.rs` : ajouter `covers_route_for`

Copier depuis `pmocache/src/lib.rs:149` :

```rust
/// Route relative d'une cover : `/covers/image/{pk}[/{param}]`
pub fn covers_route_for(pk: &str, param: Option<&str>) -> String {
    if let Some(p) = param {
        format!("/covers/image/{}/{}", pk, p)
    } else {
        format!("/covers/image/{}", pk)
    }
}
```

### Étape 4 — `pmoaudiocache/src/lib.rs` : ajouter `audio_route_for`

```rust
/// Route relative d'un fichier audio : `/audio/tracks/{pk}`
pub fn audio_route_for(pk: &str) -> String {
    format!("/audio/tracks/{}", pk)
}
```

### Étape 5 — `pmocache/src/lib.rs` : renommer `covers_absolute_url_for`

```rust
#[deprecated(note = "Utiliser covers_absolute_url_for_upnp() dans les contextes UPnP uniquement")]
pub fn covers_absolute_url_for(pk: &str, param: Option<&str>) -> String {
    covers_absolute_url_for_upnp(pk, param)
}

pub fn covers_absolute_url_for_upnp(pk: &str, param: Option<&str>) -> String {
    let base = std::env::var("PMO_SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());
    format!("{}{}", base.trim_end_matches('/'), covers_route_for(pk, param))
}
```

Mettre à jour l'appel UPnP connu : `pmoupnp/src/cache_registry.rs:57`
→ `covers_absolute_url_for(` → `covers_absolute_url_for_upnp(`

### Étape 6 — `pmoradiofrance/src/metadata_cache.rs:263` : stocker la route, pas l'URL

**Contexte** : tâche de fond — ne peut pas extraire `Extension<BaseUrl>`.
**Principe** : stocker la route relative dans le cache ; le handler rebase au moment de la réponse.

```rust
// Avant :
let public_url = pmocache::covers_absolute_url_for(&pk, None);

// Après :
let public_url = pmocovers::covers_route_for(&pk, None); // route relative
```

Le handler REST dans `pmoradiofrance/src/api_rest.rs` qui retourne ces métadonnées doit :
1. Ajouter `Extension(base_url): Extension<BaseUrl>` à sa signature
2. Construire l'URL : `base_url.url_for(&metadata.album_art_uri)`

Lire `api_rest.rs` pour identifier le handler exact qui inclut `album_art_uri` dans la réponse.

### Étape 7 — Handlers REST `pmocontrol/src/pmoserver_ext.rs`

`get_renderer_full_snapshot` (l.170) et `browse_container` (l.2080) propagent `album_art_uri`
depuis les résultats DIDL des media servers UPnP — ces URLs pointent vers l'IP du media server,
pas de pmomusic.

**Action** : après l'audit, vérifier si ces URLs passent par `covers_absolute_url_for`.
Si oui → même traitement qu'étape 6. Sinon → pas de changement.

### Étape 8 — Handlers SSE `pmocontrol/src/sse.rs`

Pour `renderer_events_sse`, `media_server_events_sse`, `all_events_sse` :

```rust
pub async fn renderer_events_sse(
    State(control_point): State<Arc<ControlPoint>>,
    Extension(base_url): Extension<BaseUrl>,  // ← ajouter
) -> impl IntoResponse {
    let base_url = base_url.clone(); // avant le stream!
    let stream = stream! {
        while let Some(event) = rx.recv().await {
            // base_url.url_for(...) pour les URLs dans les événements
        }
    };
}
```

Vérifier si les événements SSE contiennent des `album_art_uri` construits avec
`covers_absolute_url_for` ou propagés depuis le cache.
Si propagation → même traitement qu'étape 6.

### Étape 9 — Vérification finale

```bash
# Ne doit retourner aucun appel dans les handlers HTTP
grep -rn "covers_absolute_url_for[^_]" --include="*.rs"

# Ne doit retourner aucun résultat dans les handlers HTTP
grep -rn "PMO_SERVER_URL" --include="*.rs" | grep -v "pmocache\|pmoserver\|test"

# Warnings deprecated
cargo build 2>&1 | grep "deprecated"
```

### Étape 10 — Tests

```rust
#[test]
fn url_for_combines_base_and_route() {
    let b = BaseUrl("https://example.com".to_string());
    assert_eq!(b.url_for("/covers/image/abc"), "https://example.com/covers/image/abc");
}

#[test]
fn url_for_trims_trailing_slash() {
    let b = BaseUrl("https://example.com/".to_string());
    assert_eq!(b.url_for("/covers/image/abc"), "https://example.com/covers/image/abc");
}
// + tests middleware X-Forwarded-Host, fallback PMO_SERVER_URL, fallback localhost
// + test intégration : chaque endpoint frontend avec X-Forwarded-Host simulé
```

### Ordre d'exécution

1. Étape 0 — audit (confirmer la liste des call sites)
2. Étapes 3, 4 — ajouter `covers_route_for` / `audio_route_for` (sans breaking change)
3. Étape 5 — renommer + `#[deprecated]` (les warnings guident la suite)
4. Étape 1 — `BaseUrl` + `base_url_layer` dans `pmoserver`
5. Étape 2 — appliquer le layer dans `server.rs`
6. Étapes 6, 7, 8 — migrer les handlers (guidés par les warnings de compilation)
7. Étapes 9, 10 — vérification + tests

## Règle après cette modification

**Interdit** : appeler `covers_absolute_url_for_upnp()`, lire `PMO_SERVER_URL`, ou utiliser
`format!("{}/audio/flac/{}", base_url, pk)` dans un handler HTTP qui retourne du JSON au frontend.

**Obligatoire** : extraire `Extension<BaseUrl>` et utiliser :
- `base_url.url_for(&pmocovers::covers_route_for(pk, None))` pour les images
- `base_url.url_for(&pmoaudiocache::audio_route_for(pk))` pour les fichiers audio
