# Rapport : Documentation d'implémentation d'une nouvelle MusicSource

## Objectif

Créer une documentation complète et pratique pour guider l'implémentation d'une nouvelle source musicale dans l'écosystème PMOMusic.

## Travail réalisé

### 1. Analyse des sources existantes

J'ai analysé deux implémentations de référence :

- **pmoparadise/src/source.rs** : Source dynamique avec FIFO (radio streaming)
- **pmoqobuz/src/source.rs** : Source catalogue avec playlists lazy

Ainsi que la documentation du trait :

- **pmosource/README.md** : Vue d'ensemble du trait MusicSource
- **pmosource/ARCHITECTURE.md** : Architecture et design decisions

### 2. Identification des patterns principaux

Deux patterns majeurs ont été identifiés :

#### Pattern 1 : Source dynamique FIFO (Radio Paradise)

**Caractéristiques :**
- Flux continu de tracks avec capacité limitée
- Suppression automatique des plus anciens
- Callbacks sur playlists pour détecter les changements
- Notification du ContentDirectory via notifier injecté
- Adaptation des IDs playlist → schema source

**Éléments clés :**
```rust
update_counter: Arc<RwLock<u32>>
last_change: Arc<RwLock<SystemTime>>
callback_tokens: Arc<Mutex<Vec<u64>>>
container_notifier: Option<Arc<dyn Fn(&[String]) + Send + Sync>>
```

#### Pattern 2 : Source catalogue lazy (Qobuz)

**Caractéristiques :**
- Catalogue vaste avec navigation hiérarchique
- Cache lazy pour audio, eager pour covers
- Playlists créées à la demande avec TTL
- LazyProvider pour télécharger l'audio à la lecture
- Métadonnées riches stockées dans le cache

**Éléments clés :**
```rust
SourceCacheManager centralisé
QobuzLazyProvider implémentant LazyProvider
Playlists avec rôle Album et TTL de 7 jours
Adaptation IDs avec metadata source_track_id
```

### 3. Structure du document créé

Le document `Blackboard/Architecture/music_source.md` contient :

#### Table des matières
1. Vue d'ensemble
2. Structure d'une MusicSource
3. Implémentation du trait MusicSource
4. Patterns d'implémentation
5. Intégration avec l'écosystème PMOMusic
6. Checklist de mise en œuvre
7. Exemples de référence

#### Sections détaillées

**Section 1 : Vue d'ensemble**
- Définition d'une MusicSource
- Types de sources (dynamique vs statique)
- Capacités du trait

**Section 2 : Structure**
- Organisation du code
- Dépendances recommandées
- Features Cargo

**Section 3 : Implémentation du trait**
- Informations de base (name, id, default_image)
- Navigation ContentDirectory (root_container, browse, resolve_uri)
- Support FIFO (append_track, remove_oldest, update_id)
- Support statique (get_items, search)

**Section 4 : Patterns**
- Pattern 1 : Source dynamique avec FIFO (code complet)
- Pattern 2 : Source catalogue avec playlists lazy (code complet)
- Pattern 3 : Adaptation des IDs entre playlist et source

**Section 5 : Intégration écosystème**
- pmoplaylist : création et gestion de playlists
- pmoaudiocache/pmocovers via SourceCacheManager
- pmodidl : conversion vers DIDL-Lite
- LazyProvider personnalisé

**Section 6 : Checklist**
- Phase 1 : Structure de base
- Phase 2 : Navigation ContentDirectory
- Phase 3 : Résolution d'URI
- Phase 4 : Support FIFO (si dynamique)
- Phase 5 : Support statique (si catalogue)
- Phase 6 : Intégration avancée
- Phase 7 : Tests et validation

**Section 7 : Exemples de référence**
- Radio Paradise (source dynamique FIFO)
- Qobuz (source catalogue lazy)
- Schemas d'Object ID détaillés

### 4. Points techniques importants documentés

#### Schema d'Object ID

Format recommandé hiérarchique :
```
<source-id>
<source-id>:albums
<source-id>:album:<album_id>
<source-id>:track:<track_id>
<source-id>:playlist:<playlist_id>
```

Exemples concrets de Radio Paradise et Qobuz fournis.

#### Adaptation des IDs

Code complet pour adapter les items de playlist au schema de la source :
- Extraction du cache_pk depuis l'URL
- Récupération du source_track_id depuis metadata
- Reconstruction de l'ID correct
- Normalisation des URLs (relatives → absolues)
- Ajout de champs requis (genre)

#### Cache lazy vs eager

Stratégie claire :
- **Covers** : Cache eager (petit, UI en a besoin immédiatement)
- **Audio** : Cache lazy (grand, téléchargé à la demande)

#### Thread Safety

Règles explicites :
- `Arc<RwLock<>>` pour état mutable partagé
- `tokio::sync::RwLock` pour async
- Éviter `Rc<>`, `RefCell` (non thread-safe)
- Implémenter `Clone` via `Arc<>`

#### Compatibilité UPnP

Points de vigilance :
- Genre obligatoire pour certains clients (gupnp-av-cp)
- URLs absolues uniquement
- Protocol Info correct pour FLAC
- Duration au format `H:MM:SS`
- childCount optionnel mais recommandé

### 5. Code d'exemple complet

Le document contient des exemples de code complets et fonctionnels pour :

1. **Structure de base** : définition de la struct et implémentation basique
2. **Navigation** : root_container et browse avec pattern matching
3. **Résolution URI** : avec fallback cache → original
4. **FIFO** : append_track, remove_oldest, callbacks
5. **Adaptation IDs** : fonction complète d'adaptation
6. **LazyProvider** : implémentation personnalisée
7. **Conversion DIDL** : traits ToDIDLContainer et ToDIDLItem

## Couverture des besoins

### Sources couvertes

- ✅ Radio Paradise : source dynamique FIFO
- ✅ Qobuz : source catalogue lazy
- ✅ Patterns génériques applicables à d'autres sources

### Cas d'usage couverts

- ✅ Source radio/streaming live
- ✅ Source catalogue de streaming (Spotify, Deezer, etc.)
- ✅ Source bibliothèque locale
- ✅ Source playlists fixes
- ✅ Source avec authentification (via client)

### Intégrations couvertes

- ✅ pmoplaylist (FIFO et persistant)
- ✅ pmoaudiocache (cache audio)
- ✅ pmocovers (cache covers)
- ✅ SourceCacheManager (centralisé)
- ✅ LazyProvider (téléchargement lazy)
- ✅ pmodidl (DIDL-Lite)

## Limitations et améliorations futures

### Limitations actuelles

1. **Search** : Pas d'exemple détaillé de search (optionnel dans le trait)
2. **Authentification** : Mentionné mais pas d'exemple complet
3. **Multi-format** : Pas d'exemple de source supportant plusieurs formats
4. **Offline** : Pas de pattern pour source offline/synchronisation

### Améliorations possibles

1. Ajouter un exemple complet de search avec filtres
2. Documenter l'intégration avec un système d'auth OAuth
3. Ajouter un pattern pour sources multi-formats (FLAC/MP3/AAC)
4. Documenter la gestion offline avec synchronisation

## Fichiers créés

- `Blackboard/Architecture/music_source.md` : Documentation complète (15 sections, ~800 lignes)

## Conclusion

Le document créé fournit un guide complet et pratique pour implémenter une nouvelle MusicSource. Il combine :

- **Théorie** : Architecture, design patterns, principes
- **Pratique** : Code complet, exemples réels, checklist
- **Référence** : Schemas d'Object ID, intégrations, compatibilité

Un développeur peut suivre ce guide étape par étape pour créer une nouvelle source musicale compatible avec l'écosystème PMOMusic, en s'inspirant des patterns éprouvés de Radio Paradise et Qobuz.
