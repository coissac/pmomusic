**Il faut suivre les instructions générales placées dans le fichier : Blackboard/Rules.md**

# MusicBoxSource : Bibliothèque musicale universelle

Créer une **"boîte à musique"** personnelle : un catalogue unifié de morceaux provenant de n'importe quelle source (Qobuz, URLs, fichiers locaux, Radio Paradise, etc.), avec taxonomie de tags et playlists intelligentes.

---

## 🎯 Vision

### Concept

**MusicBoxSource** est une bibliothèque musicale curatoriale qui permet de :
- **Collecter** : Ajouter des morceaux depuis n'importe quelle source PMOMusic ou URL
- **Organiser** : Classifier avec une taxonomie de tags extensible
- **Requêter** : Créer des playlists statiques et smart playlists (requêtes dynamiques)
- **Exposer** : Servir via UPnP/DIDL-Lite avec navigation multi-axes

### Différence avec `pmoplaylist`

- **`pmoplaylist`** : Playlists FIFO **éphémères** pour sources live (Radio Paradise)
- **`pmomusicbox`** : Bibliothèque **persistante** cross-sources avec métadonnées enrichies

---

## 🏛️ Architecture globale

```mermaid
flowchart TB
    subgraph Sources[Sources PMOMusic]
        QOBUZ[pmoqobuz]
        PARADISE[pmoparadise]
        LOCAL[pmolocal - à créer]
        URL[URLs directes]
    end
    
    subgraph Import[Import Layer]
        IMPORTER[MusicBox Importer]
        JSPF[pmojspf - Parser playlists]
        META[pmometadata - Extraction]
    end
    
    subgraph Core[pmomusicbox Core]
        DB[(SQLite Database)]
        TAXONOMY[Taxonomie Tags]
        QUERY[Smart Query Engine]
    end
    
    subgraph Cache[Cache Layer]
        AUDIO[pmoaudiocache]
        COVERS[pmocovers]
    end
    
    subgraph Export[Export UPnP]
        SOURCE[MusicSource Trait]
        DIDL[DIDL-Lite Generator]
        BROWSE[Multi-Axis Browser]
    end
    
    Sources --> IMPORTER
    URL --> IMPORTER
    JSPF --> IMPORTER
    META --> IMPORTER
    
    IMPORTER --> DB
    DB --> TAXONOMY
    DB --> QUERY
    
    DB <--> AUDIO
    DB <--> COVERS
    
    DB --> SOURCE
    TAXONOMY --> BROWSE
    QUERY --> BROWSE
    SOURCE --> DIDL
    BROWSE --> DIDL
```

---

## 🗄️ Modèle de données (SQLite)

### Tables principales

```mermaid
erDiagram
    TAG_CATEGORIES ||--o{ TAGS : contient
    TAG_CATEGORIES ||--o{ TAG_CATEGORIES : parent
    TAGS ||--o{ ITEM_TAGS : associe
    MUSIC_ITEMS ||--o{ ITEM_TAGS : a
    MUSIC_ITEMS ||--o{ PLAYLIST_ITEMS : dans
    PLAYLISTS ||--o{ PLAYLIST_ITEMS : contient
    
    TAG_CATEGORIES {
        text id PK "Ex: mood, genre"
        text name "Nom affiché"
        text parent_id FK "Hiérarchie"
        text color "Hex color"
        text icon "Emoji/icon"
        int display_order
    }
    
    TAGS {
        text id PK "Ex: mood:energetic"
        text category_id FK
        text name "energetic, chill"
        text description
        text color "Override"
    }
    
    MUSIC_ITEMS {
        text id PK "UUID"
        text source_type "qobuz, url, local"
        text source_id "ID source"
        text original_uri "URI source"
        text cache_audio_pk FK "pmoaudiocache"
        text cache_cover_pk FK "pmocovers"
        text title
        text artist
        text album
        int year
        int rating "1-5 étoiles"
        int play_count
    }
    
    ITEM_TAGS {
        text item_id PK,FK
        text tag_id PK,FK
        int added_at
        text source "user, auto"
    }
    
    PLAYLISTS {
        text id PK
        text name
        bool is_smart
        text smart_query "JSON"
    }
    
    PLAYLIST_ITEMS {
        text playlist_id PK,FK
        text item_id FK
        int position PK
    }
```

### Tables d'association

- **`item_tags`** : Liens items ↔ tags (N:M)
- **`playlist_items`** : Items dans playlists statiques (position, ordre)
- **`tag_synonyms`** : Synonymes pour recherche (ex: "jazz" → "swing")

### Index & Recherche

- **Indexes B-tree** : artist, album, genre, year, rating, play_count
- **FTS5 (Full-Text Search)** : title, artist, album, comment
- **Triggers** : Maintien des tables FTS en sync avec `music_items`

---

## 🎨 Taxonomie par défaut

Catégories préchargées à l'initialisation :

| Catégorie   | Description                      | Exemples de tags                           |
|-------------|----------------------------------|--------------------------------------------|
| **Mood**    | État d'esprit, émotion           | energetic, chill, melancholic, happy       |
| **Genre**   | Style musical                    | rock, jazz, classical, electronic, metal   |
| **Era**     | Période, décennie                | 60s, 70s, 80s, 90s, contemporary           |
| **Occasion**| Contexte d'écoute                | workout, focus, party, driving, sleep      |
| **Tempo**   | Vitesse                          | slow, medium, fast                         |
| **Instrument** | Instrument dominant           | piano, guitar, vocal, synthesizer          |
| **Quality** | Qualité audio                    | lossless, high-res, remastered, live       |
| **Origin**  | Origine géographique             | usa, uk, france, japan, latin, africa      |

**Extensibilité** : L'utilisateur peut créer ses propres catégories et tags.

---

## 📦 Crates architecture

### 1. **`pmojspf`** - Parser de playlists (utilitaire)

**But** : Parser/écrire différents formats de playlists vers/depuis un format pivot JSPF (JSON).

```
pmojspf/
├── model.rs        # Structures JSPF (Playlist, Track, Meta)
├── reader/
│   ├── jspf.rs     # JSON natif
│   ├── xspf.rs     # XML (via quick-xml ou crate xspf)
│   ├── m3u.rs      # M3U/M3U8 (parsing ligne par ligne)
│   └── pls.rs      # PLS (format INI-like)
└── writer.rs       # Export JSPF
```

**Dépendances** : `serde`, `serde_json`, `quick-xml` (ou `xspf` crate)

**Usage** : Réutilisé par `pmomusicbox` pour import/export

---

### 2. **`pmomusicbox`** - Bibliothèque musicale core

**Responsabilités** :
- Gestion base SQLite (CRUD items, tags, playlists)
- Import depuis sources PMO (Qobuz, Paradise, Local, URLs)
- Smart playlists (query builder + exécution SQL)
- Implémentation `MusicSource` trait (exposition UPnP)
- Intégration caches audio/covers

```
pmomusicbox/
├── db/
│   ├── schema.rs       # DDL SQLite + migrations
│   ├── items.rs        # CRUD music_items
│   ├── tags.rs         # CRUD tags + taxonomie
│   ├── playlists.rs    # CRUD playlists statiques
│   ├── smart.rs        # Smart playlists
│   └── search.rs       # Full-text search (FTS5)
│
├── import/
│   ├── url.rs          # Import URL directe
│   ├── source.rs       # Import depuis MusicSource
│   ├── local.rs        # Import fichiers locaux (via pmometadata)
│   └── playlist.rs     # Import JSPF/M3U8 (via pmojspf)
│
├── export/
│   └── playlist.rs     # Export playlists (JSPF, M3U8)
│
├── query/
│   ├── builder.rs      # SmartPlaylistQuery (DSL)
│   └── executor.rs     # Génération + exécution SQL
│
├── didl/
│   └── generator.rs    # Conversion items → DIDL-Lite
│
├── source.rs           # Impl MusicSource trait
├── taxonomy.rs         # Taxonomie par défaut + CRUD
└── config_ext.rs       # Extension pmoconfig
```

**Dépendances** :
- `pmosource`, `pmoaudiocache`, `pmocovers`, `pmodidl`, `pmometadata`
- `pmojspf` (import/export playlists)
- `rusqlite` (features: `bundled`, `serde_json`)
- `uuid`, `serde`, `tokio`, `async-trait`

---

### 3. **`pmolocal`** - Source fichiers locaux (à créer)

**But** : Scanner des répertoires locaux et exposer les fichiers audio via `MusicSource`.

```
pmolocal/
├── scanner.rs      # Scan récursif de répertoires
├── watcher.rs      # Hot reload (notify)
├── source.rs       # Impl MusicSource
└── config_ext.rs   # Extension pmoconfig
```

**Workflow** :
1. `pmolocal` scanne `/home/user/Music`
2. `pmomusicbox` importe les items découverts
3. Tags automatiques basés sur métadonnées (genre, année)

---

## 🔄 Flux d'import

### Import depuis une source PMO (ex: Qobuz)

```mermaid
sequenceDiagram
    participant QS as Qobuz Source
    participant MB as MusicBox Importer
    participant DB as SQLite DB
    participant AC as pmoaudiocache
    participant CC as pmocovers
    
    QS->>MB: get_item(object_id)
    MB->>QS: resolve_uri(object_id)
    
    Note over MB: 1. Extraire métadonnées DIDL-Lite<br/>2. Générer UUID
    
    MB->>DB: INSERT INTO music_items
    
    opt Auto-cache activé
        MB->>AC: Cache audio
        MB->>CC: Cache cover
        AC-->>DB: Retourner cache_audio_pk
        CC-->>DB: Retourner cache_cover_pk
    end
    
    MB-->>QS: item_id (UUID)
```

### Import URL directe

```mermaid
flowchart LR
    URL[URL simple] --> META["pmometadata<br/>Extraction"]
    META --> UUID[Générer UUID]
    UUID --> DB[("music_items")]
    DB --> CACHE{"Auto-cache?"}
    CACHE -->|Oui| AC[pmoaudiocache]
    CACHE -->|Non| END[Fin]
    AC --> END
```

### Import playlist JSPF/M3U8

```mermaid
flowchart LR
    FILE[Fichier playlist] --> JSPF["pmojspf<br/>Parser"]
    JSPF --> STRUCT[Structure JSPF]
    STRUCT --> LOOP{"Pour chaque track"}
    LOOP --> IMPORT[Import comme URL]
    IMPORT --> DB[("music_items")]
    DB --> PLAYLIST[Créer playlist statique]
    PLAYLIST --> LINK[Lier tracks à playlist]
```

---

## 🔍 Smart Playlists (Query DSL)

### Concept

Les smart playlists sont des **requêtes sauvegardées** qui génèrent dynamiquement une liste de tracks.

### Structure de requête (JSON)

```json
{
  "include_all_tags": ["mood:energetic", "genre:rock"],
  "exclude_tags": ["mood:melancholic"],
  "year_min": 1980,
  "year_max": 1989,
  "min_rating": 4,
  "lossless_only": true,
  "order_by": "play_count",
  "order": "desc",
  "limit": 50
}
```

### Traduction SQL

```sql
SELECT * FROM music_items
WHERE id IN (
    SELECT item_id FROM item_tags WHERE tag_id IN ('mood:energetic', 'genre:rock')
    GROUP BY item_id HAVING COUNT(DISTINCT tag_id) = 2  -- ALL tags
)
AND id NOT IN (
    SELECT item_id FROM item_tags WHERE tag_id = 'mood:melancholic'
)
AND year BETWEEN 1980 AND 1989
AND rating >= 4
AND codec IN ('flac', 'alac')
ORDER BY play_count DESC
LIMIT 50;
```

---

## 🎭 Exposition UPnP (MusicSource)

### Structure de navigation

```mermaid
graph TB
    ROOT[musicbox/] --> ARTIST[by-artist/]
    ROOT --> ALBUM[by-album/]
    ROOT --> GENRE[by-genre/]
    ROOT --> TAG[by-tag/]
    ROOT --> PLAYLISTS[playlists/]
    ROOT --> SMART[smart-playlists/]
    ROOT --> FAV[favorites/]
    ROOT --> RECENT[recent/]
    
    ARTIST --> PF[Pink Floyd/]
    ARTIST --> Q[Queen/]
    PF --> WALL[The Wall/]
    PF --> WYWH[Wish You Were Here/]
    WALL --> ITEM1[Another Brick... 🎵]
    
    TAG --> MOOD[mood/]
    TAG --> OCC[occasion/]
    TAG --> ERA[era/]
    
    MOOD --> ENRG[energetic/]
    MOOD --> CHILL[chill/]
    ENRG --> ITEMS1[items taggués 🎵]
    
    OCC --> WORK[workout/]
    OCC --> FOCUS[focus/]
    
    ERA --> E80[80s/]
    ERA --> E90[90s/]
    
    PLAYLISTS --> PL1[My Favorites/]
    PLAYLISTS --> PL2[Summer 2024/]
    
    SMART --> SP1[80s Rock Workout/]
    SMART --> SP2[Jazz Dinner/]
    
    style ITEM1 fill:#e1f5ff
    style ITEMS1 fill:#e1f5ff
```

### Object IDs

```
musicbox:by-artist:{artist_name}
musicbox:by-album:{album_id}
musicbox:by-tag:{category}:{tag_name}
musicbox:playlist:{playlist_id}
musicbox:smart:{smart_playlist_id}
musicbox:item:{item_id}
```

---

## 🔌 Intégration avec l'écosystème PMOMusic

### Avec pmoaudiocache

- Import → Déclencher cache automatique (si `auto_cache: true`)
- `resolve_uri()` → Retourner URI cachée si disponible

### Avec pmocovers

- Import → Télécharger cover art
- Browse → Inclure `album_art` dans DIDL-Lite

### Avec pmoserver (feature `server`)

- API REST pour manipulation (CRUD items, tags, playlists)
- SSE pour notifications de changements
- Endpoints OpenAPI (utoipa)

---

## 📝 Plan d'implémentation (Phases)

### Phase 1 : Fondations
- Schéma SQLite complet
- Crate `pmojspf` (parser playlists)
- CRUD basique dans `pmomusicbox` (items, tags)
- Taxonomie par défaut
- Import URL simple
- Extension pmoconfig

### Phase 2 : Import cross-sources
- Import depuis MusicSource (Qobuz, Paradise)
- Import playlists (JSPF/M3U8)
- Intégration caches (audio, covers)
- Crate `pmolocal` (fichiers locaux)

### Phase 3 : Smart Playlists
- Query builder (DSL)
- Exécuteur SQL
- CRUD smart playlists
- Export JSPF

### Phase 4 : MusicSource UPnP
- Implémentation trait `MusicSource`
- Génération DIDL-Lite
- Browse multi-axes (artist, album, tag)
- Recherche full-text (FTS5)

### Phase 5 : Fonctionnalités avancées
- Statistiques d'écoute (play_count, last_played)
- Auto-tagging (genre depuis métadonnées)
- API REST (feature `server`)
- Recommandations (items similaires)

---

## 🎯 Cas d'usage

### Workflow typique

1. **Découverte** : Écouter Radio Paradise, tomber sur un morceau génial
2. **Ajout** : `musicbox.import_from_source(&paradise, "track-123")`
3. **Organisation** : Ajouter tags `mood:chill`, `occasion:focus`
4. **Playlist** : Smart playlist "Focus Music" avec requête `mood:chill + occasion:focus`
5. **Écoute** : Naviguer dans UPnP → `musicbox/smart-playlists/Focus Music/`

### Scénario : Bibliothèque mixte

- Albums Qobuz haute résolution
- Playlists M3U8 importées depuis iTunes
- Fichiers FLAC locaux scannés
- URLs de SoundCloud
- Tracks Radio Paradise capturés

**Tout unifié dans MusicBox, accessible via UPnP, organisé par tags.**

---

## 📚 Références

### Standards
- [JSPF Spec](https://www.xspf.org/jspf)
- [XSPF Spec](https://www.xspf.org/spec)
- [SQLite FTS5](https://www.sqlite.org/fts5.html)

### Inspirations
- [Beets](https://beets.io/) - Music library manager
- [Navidrome](https://www.navidrome.org/) - Music server
- [MusicBrainz Picard](https://picard.musicbrainz.org/) - Tagger
