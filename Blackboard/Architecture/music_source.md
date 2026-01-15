# Guide d'implémentation d'une nouvelle MusicSource

Ce document décrit comment implémenter une nouvelle source musicale dans l'écosystème PMOMusic en suivant le trait `MusicSource` défini dans le crate `pmosource`.

## Table des matières

1. [Vue d'ensemble](#vue-densemble)
2. [Structure d'une MusicSource](#structure-dune-musicsource)
3. [Implémentation du trait MusicSource](#implémentation-du-trait-musicsource)
4. [Patterns d'implémentation](#patterns-dimplémentation)
5. [Intégration avec l'écosystème PMOMusic](#intégration-avec-lécosystème-pmomusic)
6. [Checklist de mise en œuvre](#checklist-de-mise-en-œuvre)
7. [Exemples de référence](#exemples-de-référence)

## Vue d'ensemble

Une `MusicSource` est une abstraction qui représente une source de contenu musical dans PMOMusic. Elle peut être :

- **Dynamique (FIFO)** : Radio Paradise, streaming radio, playlists live
- **Statique** : Albums Qobuz, bibliothèque locale, playlists fixes

Le trait `MusicSource` définit une interface unifiée pour :
- La navigation UPnP ContentDirectory (browse)
- La résolution d'URI audio (avec cache)
- La gestion de playlists FIFO (pour les sources dynamiques)
- Le suivi des changements (update_id, last_change)

## Structure d'une MusicSource

### Organisation du code

```
pmo<votre-source>/
├── src/
│   ├── lib.rs              # Exports publics
│   ├── source.rs           # Implémentation MusicSource
│   ├── client.rs           # Client API (optionnel)
│   ├── models.rs           # Structures de données
│   ├── config.rs           # Configuration
│   └── didl.rs             # Conversion DIDL-Lite (optionnel)
├── assets/
│   └── default.webp        # Logo 300x300px
├── Cargo.toml
└── README.md
```

### Dépendances principales

```toml
[dependencies]
pmosource = { path = "../pmosource" }
pmodidl = { path = "../pmodidl" }
pmoplaylist = { path = "../pmoplaylist", optional = true }  # Si FIFO
pmoaudiocache = { path = "../pmoaudiocache", optional = true }  # Si cache
pmocovers = { path = "../pmocovers", optional = true }  # Si cache

async-trait = "0.1"
tokio = { version = "1", features = ["sync"] }
serde = { version = "1", features = ["derive"] }

[features]
default = ["cache"]
cache = ["pmoaudiocache", "pmocovers"]
playlist = ["pmoplaylist"]
```

## Implémentation du trait MusicSource

### 1. Informations de base

Chaque source doit fournir :

```rust
use pmosource::{async_trait, MusicSource};

#[derive(Clone, Debug)]
pub struct MyMusicSource {
    // Champs internes
}

#[async_trait]
impl MusicSource for MyMusicSource {
    fn name(&self) -> &str {
        "Ma Source Musicale"  // Nom affiché dans l'UI
    }

    fn id(&self) -> &str {
        "my-music-source"  // ID unique (format: lowercase-kebab-case)
    }

    fn default_image(&self) -> &[u8] {
        // Logo WebP 300x300px inclus dans le binaire
        include_bytes!("../assets/default.webp")
    }

    fn default_image_mime_type(&self) -> &str {
        "image/webp"  // Toujours WebP
    }
}
```

**Règles :**
- `id()` doit être unique parmi toutes les sources
- `id()` doit être en lowercase-kebab-case
- `default_image()` doit être un WebP 300x300px

### 2. Navigation ContentDirectory

#### 2.1 Container racine

```rust
async fn root_container(&self) -> Result<Container> {
    Ok(Container {
        id: self.id().to_string(),  // "my-music-source"
        parent_id: "0".to_string(),  // Toujours "0" pour la racine
        restricted: Some("1".to_string()),
        child_count: None,  // Optionnel
        searchable: Some("1".to_string()),
        title: self.name().to_string(),
        class: "object.container".to_string(),
        artist: None,
        album_art: None,
        containers: vec![],
        items: vec![],
    })
}
```

#### 2.2 Browse

La méthode `browse()` est le cœur de la navigation :

```rust
async fn browse(&self, object_id: &str) -> Result<BrowseResult> {
    match self.parse_object_id(object_id) {
        ObjectIdType::Root => {
            // Retourner les sous-containers principaux
            let containers = vec![
                self.build_albums_container(),
                self.build_playlists_container(),
                self.build_favorites_container(),
            ];
            Ok(BrowseResult::Containers(containers))
        }

        ObjectIdType::Album { album_id } => {
            // Retourner le container + ses tracks
            let album_container = self.build_album_container(&album_id);
            let tracks = self.get_album_tracks(&album_id).await?;
            Ok(BrowseResult::Mixed {
                containers: vec![album_container],
                items: tracks,
            })
        }

        ObjectIdType::Track { track_id } => {
            // Retourner les détails d'un track
            let track = self.get_track_item(&track_id).await?;
            Ok(BrowseResult::Items(vec![track]))
        }

        _ => Err(MusicSourceError::ObjectNotFound(
            format!("Unknown object: {}", object_id)
        ))
    }
}
```

**Schema d'Object ID recommandé :**

```
<source-id>                              # Racine
<source-id>:albums                       # Container albums
<source-id>:album:<album_id>             # Album spécifique
<source-id>:track:<track_id>             # Track spécifique
<source-id>:playlist:<playlist_id>       # Playlist spécifique
```

**Types de BrowseResult :**
- `Containers(Vec<Container>)` : Liste de containers (navigation)
- `Items(Vec<Item>)` : Liste de tracks (lecture)
- `Mixed { containers, items }` : Les deux (album avec tracks)

#### 2.3 Résolution d'URI

```rust
async fn resolve_uri(&self, object_id: &str) -> Result<String> {
    // Étape 1 : Vérifier le cache audio
    if let Some(cached_pk) = self.get_cached_audio_pk(object_id).await {
        return Ok(format!("{}/audio/flac/{}", self.base_url, cached_pk));
    }

    // Étape 2 : Retourner l'URI originale
    match self.parse_object_id(object_id) {
        ObjectIdType::Track { track_id } => {
            let stream_url = self.get_stream_url(&track_id).await?;
            Ok(stream_url)
        }
        _ => Err(MusicSourceError::UriResolutionError(
            format!("Cannot resolve URI for: {}", object_id)
        ))
    }
}
```

**Ordre de résolution :**
1. Cache audio local (si disponible)
2. URI originale (API streaming, fichier local, etc.)

### 3. Support FIFO (sources dynamiques)

Si votre source est dynamique (radio, streaming live) :

```rust
use pmoplaylist::PlaylistManager;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct RadioSource {
    playlist_id: String,
    update_counter: Arc<RwLock<u32>>,
    last_change: Arc<RwLock<SystemTime>>,
}

#[async_trait]
impl MusicSource for RadioSource {
    fn supports_fifo(&self) -> bool {
        true  // Cette source utilise une FIFO
    }

    async fn append_track(&self, track: Item) -> Result<()> {
        // Récupérer le gestionnaire de playlist
        let manager = PlaylistManager();
        let writer = manager
            .get_persistent_write_handle(self.playlist_id.clone())
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        // Extraire le PK depuis l'URI du track
        let pk = self.extract_pk_from_item(&track)?;

        // Ajouter à la playlist
        writer
            .push_lazy(pk)
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        // Incrémenter update_id
        self.bump_update_counter().await;

        Ok(())
    }

    async fn remove_oldest(&self) -> Result<Option<Item>> {
        let manager = PlaylistManager();
        let reader = manager
            .get_read_handle(&self.playlist_id)
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        // Récupérer le plus ancien
        let items = reader.to_items(1).await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        if let Some(item) = items.first() {
            // Adapter l'item au schéma de la source
            let adapted = self.adapt_item_to_schema(item.clone());
            self.bump_update_counter().await;
            Ok(Some(adapted))
        } else {
            Ok(None)
        }
    }

    async fn update_id(&self) -> u32 {
        *self.update_counter.read().await
    }

    async fn last_change(&self) -> Option<SystemTime> {
        Some(*self.last_change.read().await)
    }

    async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>> {
        let manager = PlaylistManager();
        let reader = manager
            .get_read_handle(&self.playlist_id)
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        // Récupérer les items
        let items = reader
            .to_items(count)
            .await
            .map_err(|e| MusicSourceError::PlaylistError(e.to_string()))?;

        // Adapter au schéma de la source
        let adapted = items.into_iter()
            .map(|item| self.adapt_item_to_schema(item))
            .collect();

        Ok(adapted)
    }
}

impl RadioSource {
    async fn bump_update_counter(&self) {
        let mut counter = self.update_counter.write().await;
        *counter = counter.wrapping_add(1).max(1);
        let mut last = self.last_change.write().await;
        *last = SystemTime::now();
    }
}
```

**Points clés :**
- Utiliser `pmoplaylist::PlaylistManager` singleton
- Incrémenter `update_id` à chaque modification
- Mettre à jour `last_change` à chaque modification
- Adapter les IDs des items au schéma de la source

### 4. Support statique (albums, bibliothèques)

Si votre source est statique (catalogue, albums) :

```rust
#[async_trait]
impl MusicSource for CatalogSource {
    fn supports_fifo(&self) -> bool {
        false  // Pas de FIFO
    }

    async fn append_track(&self, _track: Item) -> Result<()> {
        Err(MusicSourceError::NotSupported(
            "This source is read-only".to_string()
        ))
    }

    async fn remove_oldest(&self) -> Result<Option<Item>> {
        Ok(None)  // Pas de suppression
    }

    async fn update_id(&self) -> u32 {
        0  // Jamais de changement
    }

    async fn last_change(&self) -> Option<SystemTime> {
        None  // Pas de suivi des changements
    }

    async fn get_items(&self, offset: usize, count: usize) -> Result<Vec<Item>> {
        // Retourner une liste paginée depuis le catalogue
        self.get_catalog_items(offset, count).await
    }
}
```

## Patterns d'implémentation

### Pattern 1 : Source dynamique avec FIFO (Radio Paradise)

**Caractéristiques :**
- Flux continu de tracks
- Capacité limitée (50-100 tracks)
- Suppression automatique des plus anciens
- `supports_fifo() = true`

**Structure :**

```rust
#[derive(Clone)]
pub struct RadioParadiseSource {
    base_url: String,
    update_counter: Arc<RwLock<u32>>,
    last_change: Arc<RwLock<SystemTime>>,
    callback_tokens: Arc<std::sync::Mutex<Vec<u64>>>,
    container_notifier: Option<Arc<dyn Fn(&[String]) + Send + Sync>>,
}

impl RadioParadiseSource {
    // Enregistrer des callbacks sur les playlists pour notifier les changements
    pub fn attach_playlist_callbacks(self: &Arc<Self>) {
        let playlist_ids = vec![
            self.live_playlist_id(),
            self.history_playlist_id(),
        ];

        let manager = PlaylistManager();
        let mut tokens = self.callback_tokens.lock().unwrap();

        for pid in playlist_ids {
            let weak = Arc::downgrade(self);
            let pid_clone = pid.clone();
            let token = manager.register_callback(move |event| {
                if event.playlist_id == pid_clone {
                    if let Some(strong) = weak.upgrade() {
                        tokio::spawn(async move {
                            strong.bump_update_counter().await;
                            // Notifier ContentDirectory
                            if let Some(notifier) = strong.container_notifier.as_ref() {
                                notifier(&[format!("radio-paradise:history")]);
                            }
                        });
                    }
                }
            });
            tokens.push(token);
        }
    }
}
```

**Points clés :**
- Callbacks sur `pmoplaylist` pour détecter les changements
- Notification du ContentDirectory via un notifier injecté
- `update_counter` partagé via `Arc<RwLock<u32>>`

### Pattern 2 : Source catalogue avec playlists lazy (Qobuz)

**Caractéristiques :**
- Catalogue vaste (millions de tracks)
- Playlists créées à la demande
- Cache lazy (cover eager, audio lazy)
- `supports_fifo() = false`

**Structure :**

```rust
#[derive(Clone)]
pub struct QobuzSource {
    inner: Arc<QobuzSourceInner>,
}

struct QobuzSourceInner {
    client: Arc<QobuzClient>,
    cache_manager: SourceCacheManager,
    base_url: String,
    update_counter: tokio::sync::RwLock<u32>,
    last_change: tokio::sync::RwLock<SystemTime>,
}

impl QobuzSource {
    // Ajouter un track avec cache lazy
    pub async fn add_track_lazy(&self, track: &Track) -> Result<(String, String)> {
        let track_id = format!("qobuz://track/{}", track.id);
        let lazy_pk = format!("QOBUZ:{}", track.id);

        // 1. Cache cover EAGERLY (petit, UI en a besoin)
        let cached_cover_pk = if let Some(ref image_url) = track.album.as_ref()
            .and_then(|a| a.image.as_ref()) {
            self.inner.cache_manager.cache_cover(image_url).await.ok()
        } else {
            None
        };

        // 2. Préparer metadata
        let metadata = AudioMetadata {
            title: Some(track.title.clone()),
            artist: track.performer.as_ref().map(|p| p.name.clone()),
            album: track.album.as_ref().map(|a| a.title.clone()),
            duration_secs: Some(track.duration as u64),
            // ... autres champs
        };

        // 3. Cache audio LAZILY (grand, téléchargé à la demande)
        let cached_audio_pk = self
            .inner
            .cache_manager
            .cache_audio_lazy_with_provider(
                &lazy_pk,
                Some(metadata.clone()),
                cached_cover_pk.clone(),
            )
            .await?;

        // 4. Stocker metadata
        self.inner.cache_manager.update_metadata(
            track_id.clone(),
            pmosource::TrackMetadata {
                original_uri: stream_url,
                cached_audio_pk: Some(cached_audio_pk.clone()),
                cached_cover_pk,
            },
        ).await;

        Ok((track_id, cached_audio_pk))
    }

    // Créer une playlist d'album avec TTL
    async fn get_or_create_album_playlist_items(
        &self,
        album_id: &str,
        limit: usize,
    ) -> Result<Vec<Item>> {
        const ALBUM_PLAYLIST_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

        let playlist_id = format!("qobuz-album-{}", album_id);
        let playlist_manager = PlaylistManager();

        // Vérifier validité (existe ET non expirée ET non vide)
        let is_valid = self.is_album_playlist_valid(&playlist_id).await?;

        if is_valid {
            // Récupérer depuis playlist existante
            let reader = playlist_manager.get_read_handle(&playlist_id).await?;
            let items = reader.to_items(limit).await?;
            return self.adapt_playlist_items_to_qobuz(items, album_id).await;
        }

        // Créer nouvelle playlist
        let writer = playlist_manager
            .create_persistent_playlist_with_role(
                playlist_id.clone(),
                pmoplaylist::PlaylistRole::Album,
            )
            .await?;

        // Ajouter tracks avec cache lazy
        self.add_album_to_playlist(&playlist_id, album_id).await?;

        // Récupérer items
        let reader = playlist_manager.get_read_handle(&playlist_id).await?;
        let items = reader.to_items(limit).await?;
        self.adapt_playlist_items_to_qobuz(items, album_id).await
    }
}
```

**Points clés :**
- Cache lazy pour l'audio (téléchargé à la demande)
- Cache eager pour les covers (petit, UI en a besoin)
- Playlists avec TTL (7 jours)
- `LazyProvider` pour télécharger l'audio lors de la lecture

### Pattern 3 : Adaptation des IDs entre playlist et source

Lorsqu'une source utilise `pmoplaylist`, les items retournés ont des IDs génériques. Il faut les adapter au schéma de la source :

```rust
async fn adapt_playlist_items_to_source(
    &self,
    items: Vec<Item>,
    parent_id: &str,
) -> Result<Vec<Item>> {
    let mut adapted = Vec::with_capacity(items.len());

    for mut item in items {
        // Extraire cache_pk depuis l'URL du resource
        let cache_pk = if let Some(resource) = item.resources.first() {
            resource
                .url
                .strip_prefix("/audio/flac/")
                .map(|s| s.to_string())
        } else {
            None
        };

        if let Some(pk) = cache_pk {
            // Récupérer source_track_id depuis metadata
            if let Ok(Some(track_id_value)) = self
                .cache_manager
                .get_audio_metadata(&pk, "source_track_id")
            {
                if let Some(track_id) = track_id_value.as_str() {
                    item.id = format!("my-source:track:{}", track_id);
                }
            }

            // Convertir URL relative en absolue
            if let Some(resource) = item.resources.first_mut() {
                if resource.url.starts_with('/') {
                    resource.url = format!("{}{}", self.base_url, resource.url);
                }
            }
        }

        item.parent_id = parent_id.to_string();

        // Normaliser album art
        if let Some(art) = item.album_art.as_mut() {
            if art.starts_with('/') {
                *art = format!("{}{}", self.base_url, art);
            }
        } else {
            item.album_art = Some(self.default_cover_url());
        }

        // Ajouter genre par défaut si absent (requis par certains clients)
        if item.genre.is_none() {
            item.genre = Some("Music".to_string());
        }

        adapted.push(item);
    }

    Ok(adapted)
}
```

**Points clés :**
- Stocker `source_track_id` dans les metadata du cache audio
- Reconstituer l'ID correct lors de la récupération depuis playlist
- Normaliser URLs (relatives → absolues)
- Ajouter champs requis par certains clients UPnP

## Intégration avec l'écosystème PMOMusic

### Avec pmoplaylist

Pour les sources dynamiques et les catalogues :

```rust
use pmoplaylist::{PlaylistManager, PlaylistRole};

// Créer une playlist persistante
let manager = PlaylistManager();
let writer = manager
    .create_persistent_playlist_with_role(
        "my-source-album-123".to_string(),
        PlaylistRole::Album,
    )
    .await?;

// Configurer metadata
writer.set_title("Album Title".to_string()).await?;
writer.set_artist(Some("Artist Name".to_string())).await?;
writer.set_cover_pk(Some("cover-pk".to_string())).await?;

// Ajouter tracks avec cache lazy
writer.push_lazy_batch(vec!["pk1", "pk2", "pk3"]).await?;

// Activer mode lazy (lookahead 2 tracks)
manager.enable_lazy_mode("my-source-album-123", 2);
```

### Avec pmoaudiocache et pmocovers (via SourceCacheManager)

```rust
use pmosource::SourceCacheManager;

// Créer le manager centralisé
let cache_manager = SourceCacheManager::from_registry("my-source".to_string())?;

// Enregistrer un LazyProvider
cache_manager.register_lazy_provider(Arc::new(MyLazyProvider::new(client)));

// Cache eager (cover)
let cover_pk = cache_manager.cache_cover("https://example.com/cover.jpg").await?;

// Cache lazy (audio)
let audio_pk = cache_manager
    .cache_audio_lazy_with_provider(
        "MY-SOURCE:123",  // Lazy PK
        Some(metadata),
        Some(cover_pk),
    )
    .await?;

// Récupérer metadata
let value = cache_manager.get_audio_metadata(&audio_pk, "key").await?;
```

**LazyProvider personnalisé :**

```rust
use pmoaudiocache::{LazyProvider, LazyProviderError};

pub struct MyLazyProvider {
    client: Arc<MyClient>,
}

#[async_trait]
impl LazyProvider for MyLazyProvider {
    async fn fetch_audio(&self, lazy_pk: &str) -> Result<Vec<u8>, LazyProviderError> {
        // Extraire l'ID depuis le lazy_pk
        let id = lazy_pk
            .strip_prefix("MY-SOURCE:")
            .ok_or_else(|| LazyProviderError::InvalidKey)?;

        // Récupérer l'URL de streaming
        let stream_url = self.client.get_stream_url(id).await
            .map_err(|e| LazyProviderError::FetchFailed(e.to_string()))?;

        // Télécharger l'audio
        let response = reqwest::get(&stream_url).await
            .map_err(|e| LazyProviderError::FetchFailed(e.to_string()))?;

        let bytes = response.bytes().await
            .map_err(|e| LazyProviderError::FetchFailed(e.to_string()))?;

        Ok(bytes.to_vec())
    }
}
```

### Avec pmodidl

Conversion de vos structures en DIDL-Lite :

```rust
use pmodidl::{Container, Item, Resource};

// Container
pub trait ToDIDLContainer {
    fn to_didl_container(&self, parent_id: &str) -> Result<Container>;
}

impl ToDIDLContainer for MyAlbum {
    fn to_didl_container(&self, parent_id: &str) -> Result<Container> {
        Ok(Container {
            id: format!("my-source:album:{}", self.id),
            parent_id: parent_id.to_string(),
            restricted: Some("1".to_string()),
            child_count: self.tracks_count.map(|c| c.to_string()),
            searchable: Some("1".to_string()),
            title: self.title.clone(),
            class: "object.container.album.musicAlbum".to_string(),
            artist: Some(self.artist.name.clone()),
            album_art: self.cover_url.clone(),
            containers: vec![],
            items: vec![],
        })
    }
}

// Item
pub trait ToDIDLItem {
    fn to_didl_item(&self, parent_id: &str) -> Result<Item>;
}

impl ToDIDLItem for MyTrack {
    fn to_didl_item(&self, parent_id: &str) -> Result<Item> {
        Ok(Item {
            id: format!("my-source:track:{}", self.id),
            parent_id: parent_id.to_string(),
            restricted: Some("1".to_string()),
            title: self.title.clone(),
            creator: self.artist.as_ref().map(|a| a.name.clone()),
            class: "object.item.audioItem.musicTrack".to_string(),
            artist: self.artist.as_ref().map(|a| a.name.clone()),
            album: self.album.as_ref().map(|a| a.title.clone()),
            genre: Some("Music".to_string()),
            album_art: self.cover_url.clone(),
            album_art_pk: self.cover_pk.clone(),
            date: self.release_date.clone(),
            original_track_number: Some(self.track_number),
            resources: vec![Resource {
                protocol_info: "http-get:*:audio/flac:*".to_string(),
                bits_per_sample: self.bit_depth.map(|b| b.to_string()),
                sample_frequency: self.sample_rate.map(|s| s.to_string()),
                nr_audio_channels: Some("2".to_string()),
                duration: self.duration_as_upnp_format(),
                url: format!("/audio/flac/{}", self.cache_pk),
            }],
            descriptions: vec![],
        })
    }
}
```

## Checklist de mise en œuvre

### Phase 1 : Structure de base

- [ ] Créer le crate `pmo<votre-source>`
- [ ] Ajouter les dépendances dans `Cargo.toml`
- [ ] Créer le logo WebP 300x300px dans `assets/`
- [ ] Définir la structure principale
- [ ] Implémenter `name()`, `id()`, `default_image()`

### Phase 2 : Navigation ContentDirectory

- [ ] Définir le schéma d'Object ID
- [ ] Implémenter `root_container()`
- [ ] Implémenter `browse()` pour la racine
- [ ] Implémenter `browse()` pour les sous-containers
- [ ] Implémenter `browse()` pour les items
- [ ] Tester la navigation avec un client UPnP

### Phase 3 : Résolution d'URI

- [ ] Implémenter `resolve_uri()` avec fallback
- [ ] Intégrer avec `SourceCacheManager`
- [ ] Implémenter `LazyProvider` si cache lazy
- [ ] Tester la lecture audio

### Phase 4 : Support FIFO (si dynamique)

- [ ] Décider de la stratégie FIFO
- [ ] Implémenter `supports_fifo() = true`
- [ ] Implémenter `append_track()`
- [ ] Implémenter `remove_oldest()`
- [ ] Implémenter `update_id()` et `last_change()`
- [ ] Enregistrer callbacks sur playlists
- [ ] Tester ajout/suppression de tracks

### Phase 5 : Support statique (si catalogue)

- [ ] Implémenter `supports_fifo() = false`
- [ ] Implémenter `get_items()` avec pagination
- [ ] Implémenter `search()` si applicable
- [ ] Tester browsing du catalogue

### Phase 6 : Intégration avancée

- [ ] Implémenter `get_item()` pour metadata
- [ ] Implémenter `capabilities()`
- [ ] Implémenter `get_available_formats()`
- [ ] Ajouter gestion d'erreurs robuste
- [ ] Documenter le code

### Phase 7 : Tests et validation

- [ ] Écrire tests unitaires
- [ ] Écrire tests d'intégration
- [ ] Tester avec différents clients UPnP
- [ ] Valider les performances
- [ ] Documenter les limitations

## Exemples de référence

### Radio Paradise (source dynamique FIFO)

**Fichier :** `pmoparadise/src/source.rs`

**Points d'intérêt :**
- Structure avec `Arc<RwLock<>>` pour l'état partagé
- Callbacks sur playlists pour détecter les changements
- Notifier injecté pour ContentDirectory
- Adaptation des IDs playlist → Radio Paradise
- Support de 4 canaux avec sous-containers

**Schema d'Object ID :**
```
radio-paradise                                    # Racine
radio-paradise:channel:{slug}                     # Canal (main, mellow, rock, eclectic)
radio-paradise:channel:{slug}:live                # Stream live
radio-paradise:channel:{slug}:liveplaylist        # Playlist live (queue)
radio-paradise:channel:{slug}:liveplaylist:track:{pk}  # Track dans queue
radio-paradise:channel:{slug}:history             # Historique
radio-paradise:channel:{slug}:history:track:{pk}  # Track dans historique
```

### Qobuz (source catalogue avec playlists lazy)

**Fichier :** `pmoqobuz/src/source.rs`

**Points d'intérêt :**
- `SourceCacheManager` centralisé
- Cache lazy pour audio, eager pour covers
- `LazyProvider` personnalisé
- Playlists d'albums avec TTL (7 jours)
- Adaptation IDs playlist → Qobuz
- Navigation hiérarchique complexe (Discover, Genres, Favorites)

**Schema d'Object ID :**
```
qobuz                                    # Racine
qobuz:discover                           # Discover Catalog
qobuz:discover:albums:ideal              # Albums (Ideal Discography)
qobuz:discover:artists                   # Artistes Featured
qobuz:genres                             # Discover Genres
qobuz:genre:{id}                         # Genre spécifique
qobuz:genre:{id}:new-releases            # Nouveautés du genre
qobuz:favorites                          # My Music
qobuz:favorites:albums                   # Albums favoris
qobuz:album:{id}                         # Album spécifique
qobuz:track:{id}                         # Track spécifique
qobuz:playlist:{id}                      # Playlist spécifique
qobuz:artist:{id}                        # Artiste spécifique
```

## Conseils d'implémentation

### Performance

1. **Cache agressif** : Utilisez `SourceCacheManager` pour tout
2. **Pagination** : Limitez le nombre d'items retournés (max 100)
3. **Lazy loading** : Ne chargez que ce qui est demandé
4. **Rate limiting** : Respectez les limites API de la source
5. **Arc<>** : Partagez les données coûteuses

### Compatibilité UPnP

1. **Genre obligatoire** : Certains clients (gupnp-av-cp) requièrent `<upnp:genre>`
2. **URLs absolues** : Toujours retourner des URLs complètes (pas de chemins relatifs)
3. **Protocol Info** : Utilisez `http-get:*:audio/flac:*` pour FLAC
4. **Duration** : Format `H:MM:SS` (ex: `0:03:45`)
5. **childCount** : Optionnel mais recommandé pour l'UI

### Gestion d'erreurs

1. **ObjectNotFound** : ID invalide
2. **BrowseError** : Erreur générique de navigation
3. **UriResolutionError** : Impossible de résoudre l'URI
4. **PlaylistError** : Erreur d'interaction avec pmoplaylist
5. **CacheError** : Erreur de cache

### Thread Safety

1. **Arc<RwLock<>>** : Pour l'état mutable partagé
2. **tokio::sync::RwLock** : Pour l'async
3. **Éviter Rc<>** : Pas thread-safe
4. **Clone** : Implémentez `Clone` pour `Arc<>`

## Conclusion

L'implémentation d'une nouvelle `MusicSource` suit ces étapes :

1. **Définir le schéma d'Object ID** : Hiérarchie claire et cohérente
2. **Implémenter la navigation** : `browse()` pour tous les niveaux
3. **Résoudre les URIs** : Cache local d'abord, puis original
4. **Gérer le cache** : `SourceCacheManager` + `LazyProvider`
5. **Adapter les IDs** : Playlist → Schema de la source
6. **Notifier les changements** : `update_id` + callbacks

Les exemples Radio Paradise et Qobuz couvrent les deux patterns principaux :
- **Dynamique FIFO** : Radio Paradise
- **Catalogue lazy** : Qobuz

En suivant ces patterns, vous obtiendrez une source musicale performante, compatible UPnP, et bien intégrée dans l'écosystème PMOMusic.
