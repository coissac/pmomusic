# Play From URL — source `UrlSource`

Inspiré par BubbleUPnP : recevoir n'importe quelle URL (lien de partage Qobuz,
flux radio, playlist M3U, page web contenant de l'audio…) et la jouer
immédiatement sur le renderer actif.

---

## Vision architecturale

`UrlSource` est une **source musicale ordinaire** qui implémente `MusicSource`,
exactement comme Qobuz, RadioFrance ou RadioParadise. Elle apparaît dans le
drawer gauche au même titre que les autres sources du serveur PMO.

Sa particularité : sa **barre de recherche est le champ URL**. L'utilisateur
colle ou tape une URL, appuie sur Entrée — la source résout l'URL et retourne
le contenu jouable comme un `BrowseResult` normal.

```
Drawer gauche
  └─ PMO Music Server
       ├─ Qobuz
       ├─ Radio Paradise
       ├─ Radio France
       └─ URL / Partage        ← nouvelle source
            └─ [barre de recherche = champ URL]
                 └─ coller une URL + Entrée
                      └─ résolution → BrowseResult → queue + play
```

Avantages de cette approche :
- **Zéro nouvelle UI** : la barre de recherche existante du drawer gère tout
- **Zéro nouvel endpoint REST** : browse/search existants suffisent
- **Zéro cas particulier** dans le drawer ou le content directory handler
- `browse()` du container racine peut afficher un **historique** des URLs jouées

---

## Trait `UrlHandler` (dans `pmosource`)

Chaque source (et un handler générique) peut revendiquer les URLs qu'elle sait
résoudre.

```rust
pub enum ResolvedContent {
    /// Référence à un container d'une source existante
    /// → la UrlSource délègue le browse à cette source
    SourceContainer {
        source_id: String,      // "qobuz", "radiofrance", …
        container_id: String,   // "qobuz:album:l46fxnqnxp5vs"
    },
    /// Liste de tracks (M3U, PLS, XSPF, RSS/podcast…)
    Playlist {
        title: Option<String>,
        items: Vec<ResolvedTrack>,
    },
    /// Track unique ou flux continu
    Track {
        uri: String,
        metadata: TrackMetadata,
    },
    Stream {
        uri: String,
        metadata: StreamMetadata,
    },
}

#[async_trait]
pub trait UrlHandler: Send + Sync {
    fn name(&self) -> &str;
    /// Priorité : plus grand = essayé en premier (défaut 50)
    fn priority(&self) -> u8 { 50 }
    /// Test rapide sans I/O (regex sur l'URL)
    fn can_handle(&self, url: &str) -> bool;
    /// Résolution effective (I/O autorisé)
    async fn resolve(&self, url: &str) -> Result<ResolvedContent, UrlResolverError>;
}
```

---

## Handlers spécifiques aux sources

### `QobuzUrlHandler` (dans `pmoqobuz`) — priorité 90

URLs reconnues. Les IDs sont potentiellement alphanumériques pour tous les
types (pas seulement les albums) :

| Forme d'URL | Exemple |
|---|---|
| `open.qobuz.com/album/<id>` | `https://open.qobuz.com/album/l46fxnqnxp5vs` |
| `play.qobuz.com/album/<id>` | `https://play.qobuz.com/album/l46fxnqnxp5vs` |
| `open.qobuz.com/track/<id>` | `https://open.qobuz.com/track/48471123` |
| `open.qobuz.com/playlist/<id>` | `https://open.qobuz.com/playlist/63246908` |
| `open.qobuz.com/artist/<id>` | `https://open.qobuz.com/artist/125709` |

Regex d'extraction : `[a-zA-Z0-9]+` pour tous les types sans exception.

Résolution sans appel API — l'ID est directement mappé sur un container_id :

```
open.qobuz.com/album/l46fxnqnxp5vs
  → ResolvedContent::SourceContainer {
        source_id: "qobuz",
        container_id: "qobuz:album:l46fxnqnxp5vs",
    }
```

### `RadioFranceUrlHandler` (dans `pmoradiofrance`) — priorité 90

URLs `radiofrance.fr/*`, `francemusique.fr/*`, `fip.fr/*`, etc.
→ `ResolvedContent::Stream`

### `RadioParadiseUrlHandler` (dans `pmoparadise`) — priorité 90

URLs `radioparadise.com/*`
→ `ResolvedContent::Stream`

---

## Handler générique (dans `pmourlresolver`, nouveau crate) — priorité 10

Dernier recours. Pipeline interne :

```
URL
 │
 ├─ Garde-fou SSRF : rejeter si IP résolue est privée/locale
 │   (RFC-1918 : 10/8, 172.16/12, 192.168/16 ; loopback : 127/8, ::1 ;
 │    link-local : 169.254/16, fe80::/10)
 │   → aucun cas d'usage légitime pour une URL interne ici
 │
 ├─ HEAD request → Content-Type audio/* ?
 │    └─ → ResolvedContent::Stream / Track (URI directe)
 │
 ├─ Extension ou Content-Type playlist ?
 │    ├─ .m3u / .m3u8 / application/vnd.apple.mpegurl → parse M3U
 │    ├─ .pls / audio/x-scpls                         → parse PLS
 │    └─ .xspf / application/xspf+xml                 → parse XSPF
 │
 ├─ application/rss+xml / application/xml ?
 │    └─ → parse RSS, extraire les <enclosure> audio → Playlist
 │
 └─ text/html ?
      └─ GET + parse HTML
           ├─ <audio src="...">
           ├─ <link type="application/rss+xml">  → RSS/Podcast
           ├─ og:audio
           └─ JSON-LD @type MusicRecording / MusicAlbum
```

Pas de yt-dlp ni de dépendance Python externe — hors scope.

---

## `UrlSource` — implémentation de `MusicSource`

```rust
pub struct UrlSource {
    resolver: UrlResolver,        // registre des handlers
    history: Arc<RwLock<VecDeque<HistoryEntry>>>,  // dernières URLs
}
```

### `name()` / `id()`

```rust
fn name(&self) -> &str { "URL / Partage" }
fn id(&self)   -> &str { "url" }
```

### `root_container()`

Retourne un container dont le contenu (`browse("url")`) est l'historique des
dernières URLs résolues avec succès (titre, source résolue, date).

### `search(query)` — cœur de la fonctionnalité

`query.text` est l'URL collée par l'utilisateur.

```
search(url)
  │
  ├─ resolver.resolve(url)
  │
  └─ match ResolvedContent
       ├─ SourceContainer { source_id, container_id }
       │    → get_source(source_id) → source.browse(container_id)
       │    → retourner le BrowseResult tel quel
       │    → ajouter à l'historique
       │
       ├─ Playlist { items }
       │    → construire un BrowseResult::Items depuis les tracks
       │    → ajouter à l'historique
       │
       ├─ Track / Stream
       │    → BrowseResult::Items avec un seul item
       │    → ajouter à l'historique
       │
       └─ Err → BrowseResult vide + log
```

Pour la délégation `SourceContainer`, `UrlSource` accède au `SOURCE_REGISTRY`
global (déjà disponible dans `pmosource`). Elle est enregistrée après les autres
sources donc elles sont toutes présentes au moment de la résolution.

### `browse(container_id)`

- `"url"` → liste de l'historique (containers/items)
- `"url:history:<n>"` → détail d'une entrée historique (si Playlist)

---

## Initialisation dans `pmomediaserver`

```rust
// Après enregistrement de Qobuz, RadioFrance, RadioParadise…

let mut resolver = UrlResolver::new();
resolver.register(Arc::new(QobuzUrlHandler::new()));
resolver.register(Arc::new(RadioFranceUrlHandler::new()));
resolver.register(Arc::new(RadioParadiseUrlHandler::new()));
resolver.register(Arc::new(GenericUrlHandler::new()));   // toujours en dernier

let url_source = Arc::new(UrlSource::new(resolver));
register_source(url_source).await;
```

---

## Points d'entrée

Le pipeline de résolution (`UrlResolver`) est le même quel que soit le point
d'entrée. Deux modes complémentaires :

### Mode "pull" — le drawer

1. L'utilisateur ouvre le drawer gauche → voit "URL / Partage" dans la liste
2. Il entre dedans → voit l'historique et la barre avec placeholder "Coller une URL…"
3. Il colle `https://open.qobuz.com/album/l46fxnqnxp5vs` + Entrée
4. Le drawer affiche les tracks de l'album (délégation Qobuz transparente)
5. Il clique ▶ sur un track ou l'album entier → lecture normale

Aucune modification du drawer nécessaire.

### Mode "push" — endpoint REST + Web Share Target (Android)

Endpoint REST dans `pmocontrol` :

```
POST /api/play-url
{ "url": "https://open.qobuz.com/album/l46fxnqnxp5vs" }
```

Résout l'URL via `UrlResolver` → ajoute au renderer actif → lecture immédiate.
Pas de navigation dans le drawer, pas de clic supplémentaire.

**Web Share Target (PWA)** — intégration dans le share sheet Android :

```json
// manifest.json
"share_target": {
  "action": "/share",
  "method": "GET",
  "params": { "url": "url" }
}
```

La page `/share?url=...` appelle l'endpoint REST et se ferme. Depuis n'importe
quelle application Android (Qobuz, navigateur, Spotify…) : menu "Partager" →
choisir PMOMusic → l'album/track joue immédiatement sur le renderer courant,
exactement comme BubbleUPnP.

Le renderer "courant" est celui qui est sélectionné dans la session active.
Pour une PWA installée sur Android, c'est la session de l'utilisateur
qui a installé l'app. Si plusieurs renderers sont disponibles, l'endpoint
peut prendre un paramètre optionnel `renderer_id` pour cibler explicitement.

---

## Plan d'implémentation

### Étape 1 — Trait + QobuzUrlHandler + UrlSource minimale

- [ ] Ajouter `UrlHandler`, `ResolvedContent`, `UrlResolver` dans `pmosource`
- [ ] Implémenter `QobuzUrlHandler` dans `pmoqobuz` (regex + mapping container_id)
- [ ] Implémenter `UrlSource` avec `search()` gérant `SourceContainer`
- [ ] Enregistrer dans `pmomediaserver`
- [ ] Tester : coller un lien Qobuz → album joue

### Étape 2 — Formats de playlist directs

- [ ] Nouveau crate `pmourlresolver` avec `GenericUrlHandler`
- [ ] Garde-fou SSRF (`is_safe_url()`)
- [ ] Détection Content-Type + parse M3U, PLS, XSPF
- [ ] `UrlSource::search()` gère `Playlist` et `Track/Stream`

### Étape 3 — Scraper HTML + historique

- [ ] Parse HTML : `<audio>`, og:audio, JSON-LD, RSS
- [ ] Historique dans `UrlSource::browse()`
- [ ] Placeholder adapté dans la barre de recherche du drawer

---

## Questions ouvertes

**Q1 — Barre de recherche : placeholder contextuel**
Quand l'utilisateur est dans "URL / Partage", le placeholder devrait afficher
"Coller une URL…" plutôt que "Rechercher…". Le drawer peut-il adapter le
placeholder selon la source active ? À voir si c'est utile en pratique (le
titre de la source dans le header est déjà indicatif).

**Q2 — Redirections**
Les liens de partage mobiles Qobuz peuvent être des URLs raccourcies. Suivre
les redirections automatiquement (reqwest le fait avec
`redirect::Policy::limited(5)`).

**Q3 — Validation Qobuz**
Le `QobuzUrlHandler` retourne un `SourceContainer` sans vérifier que l'album
existe ou est accessible. L'erreur éventuelle sera levée au moment du browse
délégué à `QobuzSource`. C'est acceptable : l'erreur arrivera rapidement avec
un message clair.
