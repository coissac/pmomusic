** Tu dois suivre scrupuleusement les règles définies dans le fichier [@Rules.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules.md) **


** Cette tâche n'est pas une tâche de codage, c'est une tâche de réflexion. Elle doit conduire à la rédaction d'un rapport dans le répertoire [@ToThinkAbout](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/ToThinkAbout) **

à parir de la page [web](https://www.radiofrance.fr/franceculture) peux-tu comprendre comment elle obtient les informations prsésenté ci-dessous:

```html
<div role="heading" aria-level="1" slot="title" class="CoverRadio-title qg-tt3 svelte-1thibul"><!----><span class="truncate qg-focus-container svelte-1t7i9vq"><!----><a href="/franceculture/podcasts/le-journal-de-l-eco/le-jouet-profite-de-la-morosite-ambiante-4949584" aria-label="Le Journal de l'éco • Le jouet profite de la morosité ambiante" data-testid="link" class="svelte-1t7i9vq underline-hover"><!----><!---->Le Journal de l'éco • Le jouet profite de la morosité ambiante<!----></a><!----></span><!----></div>
```

et

```html
<p class="CoverRadio-subtitle qg-tt5 qg-focus-container svelte-1thibul" slot="subtitle"><!----><!----><!----><a href="/franceculture/podcasts/les-matins" data-testid="link" class="svelte-1t7i9vq"><!---->Les Matins<!----></a><!----> <span class="CoverRadio-producer qg-tx1 svelte-qz676b">par Guillaume Erner</span><!----><!----></p>
```

## Round 2:

À partir de [@api_radiofrance_complete.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/ToThinkAbout/api_radiofrance_complete.md) et de [@music_source.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Architecture/music_source.md) 

Nous allons commencer l'implémentation de pmoradiofrance en implémentant dans un fichier client.rs Une API de requêtes sur Radio France. Pour les fonctionnalités, il faut effectivement se reporter au fichier `api_radiofrance_complete.md` Et pour l'architecture, il est possible de s'inspirer de [@client.rs](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoparadise/src/client.rs)

## Round 3

A partir des découvertes faites durant le round 2, Tu as maintenant le droit d'écrire du code. Tu peux donc écrire le fichier: client.rs de la nouvelle crate pmoradiofrance. Pour cela, tu devras t'appuyer sur les rapports [@api_radiofrance_complete.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/ToThinkAbout/api_radiofrance_complete.md) et de [@music_source.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Architecture/music_source.md) 

Il faut absolument cacher les réponses de l'API, Afin de limiter au maximum les requêtes inutiles, Notamment, on sait que les chaînes et les web radios ne changent que très rarement. On peut peut-être penser à faire une extension de configuration [@pmoconfig_ext.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Architecture/pmoconfig_ext.md) Pour stocker les informations principales ainsi qu'un timestamp et se dire que sauf requêtes forcées on ne va pas mettre à jour cette liste plus d'une fois par semaine.

## Round 4

Je pense qu'il faut maintenant implémenter le deuxième niveau de la Crate Radio France, En implémentant un client Stateful qui peut retourner facilement des listes de radio, des playlists, qui charge ces informations soit à part de la configuration, soit à partir de l'API, suivant que le TTL est dépassé ou pas. Le tout pour préparer la construction de la source Radio France.

Pensez comme rêgle métier à renommer les choses qui apparaissent comme `France Bleue` en `ICI` Au niveau des labels d'affichage, pas des slugs évidemment.

Voilà le type d'arborescence de browsing qu'on pourrait avoir.
On ne présente à chaque fois qu'un lien vers le flux de plus haute résolution.


```
Radio France
├── France Culture
├── FIP
│   ├── FIP
│   ├── FIP Cultes
│   ├── FIP Nouveautes
│   ├── FIP ...
│   └── FIP Pop
├── Mouv'
├── ...
└── ICI
    ├── ICI Alsace
    ├── ICI Armorique
    ├── ICI Auxerre 
    ├── ... 
    └── ICI Vaucluse
```

Sous le folder principal Radio France, On doit avoir une playlist par station. Les stations qui n'ont qu'une seule chaîne ne contiennent que cette chaîne dans leur PMOplaylist, Les autres, si elles ont une station principale comme FIP, commencent par cette station principale puis leur station annexe.
Évidemment, normalement, le contenu en titre des playlists ne doit pas évoluer. Mais les métadonnées si régulièrement Si l'on met le titre de la station comme équivalent d'un titre d'album, Le nom de l'émission pourrait être l'auteur. Et le titre de l'émission du jour, le titre du morceau. Ainsi, au fur et à mesure du temps, on fait évoluer des métadonnées pour changer la couverture, le nom de l'émission, le titre de l'émissions du jour.

Je pense que les playlists doivent être des playlists volatiles. Les covers doivent être cachés dans [@pmocovers](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmocovers) Par contre les URL doivent être passées telles qu'elles, C'est du pur stream, on ne va pas les cacher dans le PMOaudiocache.

étend le document de réflexion pour proposer une architecture: [@api_radiofrance_complete.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/ToThinkAbout/api_radiofrance_complete.md)

## Round 4bis : Implémentation du Client Stateful

**Crate concernée** : `pmoradiofrance`

**Objectif** : Implémenter le client stateful et les structures de support pour Radio France selon l'architecture définie dans le Round 4 de `api_radiofrance_complete.md`.

### Fichiers à créer/modifier

#### 1. `pmoradiofrance/src/stateful_client.rs` (NOUVEAU)

Implémenter le client stateful `RadioFranceStatefulClient` qui :

**Responsabilités** :
- Gère le cache des stations découvertes (via pmoconfig)
- Gère le cache mémoire des métadonnées live (HashMap avec TTL)
- Expose des méthodes de haut niveau pour la découverte et l'organisation des stations
- Construit des playlists UPnP à partir des métadonnées live
- Intègre avec `SourceCacheManager` pour les covers

**Structure principale** :
```rust
pub struct RadioFranceStatefulClient {
    client: RadioFranceClient,
    config: Arc<Config>,
    metadata_cache: Arc<RwLock<HashMap<String, LiveMetadataCache>>>,
    cache_manager: SourceCacheManager,
}
```

**Méthodes à implémenter** :

**Discovery avec cache** :
- `new()` - Créer depuis config par défaut
- `with_client_and_config()` - Créer avec client et config personnalisés
- `get_all_stations()` - Retourne toutes les stations (cache si valide, sinon découverte)
- `refresh_stations()` - Force la redécouverte (ignore cache)
- `get_main_stations()` - Filtre les stations principales
- `get_webradios(parent)` - Filtre les webradios d'une station parent
- `get_local_radios()` - Filtre les radios locales ICI

**Organisation hiérarchique** :
- `get_stations_by_group()` - Retourne `StationGroups` avec regroupement logique
  - `standalone` : Stations sans webradios (France Culture, France Inter, France Info)
  - `with_webradios` : Groupes FIP et France Musique avec leurs webradios
  - `local_radios` : Toutes les radios ICI (France Bleu)

**Métadonnées live** :
- `get_live_metadata(station)` - Retourne métadonnées (cache court terme si valide)
- `refresh_live_metadata(station)` - Force le rafraîchissement (ignore cache)

**Construction de playlists** :
- `build_station_playlist(station)` - Construit une playlist UPnP complète pour une station
- `update_playlist_metadata(station, playlist)` - Met à jour les métadonnées volatiles
- `get_stream_url(station)` - Retourne l'URL du stream HiFi

**Helpers** :
- `is_station_cache_valid()` - Vérifie validité du cache des stations
- `station_cache_age_secs()` - Retourne l'âge du cache en secondes

**Implémentation du cache mémoire** :
```rust
struct LiveMetadataCache {
    metadata: LiveResponse,
    fetched_at: SystemTime,
    valid_until: SystemTime,
}
```

**Logique de cache des métadonnées live** :
1. Vérifier HashMap en mémoire
2. Si présent et `SystemTime::now() < valid_until` : retourner cache
3. Sinon : appeler `client.live_metadata()`, calculer `valid_until` depuis `delayToRefresh`, stocker, retourner

#### 2. `pmoradiofrance/src/playlist.rs` (NOUVEAU)

Structures et helpers pour la construction de playlists UPnP.

**Important** : Utiliser les structures `Item` et `Resource` de `pmodidl` pour représenter les playlists UPnP.

**Structures à définir** :

```rust
use pmodidl::{Item, Resource};

/// Groupes de stations organisés hiérarchiquement
pub struct StationGroups {
    pub standalone: Vec<Station>,
    pub with_webradios: Vec<StationGroup>,
    pub local_radios: Vec<Station>,
}

/// Groupe station principale + webradios
pub struct StationGroup {
    pub main: Station,
    pub webradios: Vec<Station>,
}

/// Playlist UPnP pour une station (volatiles)
/// 
/// Contient UN SEUL item pmodidl::Item représentant le stream.
/// Les métadonnées de l'item changent au fil du temps (émissions, morceaux)
/// mais l'URL du stream reste identique.
pub struct StationPlaylist {
    /// ID de la playlist (ex: "radiofrance:franceculture")
    pub id: String,
    
    /// Station source
    pub station: Station,
    
    /// Item UPnP unique représentant le stream
    /// Utilise pmodidl::Item avec toutes les métadonnées volatiles
    pub stream_item: Item,
}
```

**Helpers à implémenter** :

```rust
impl StationPlaylist {
    /// Construire une playlist depuis les métadonnées live
    /// 
    /// Crée un pmodidl::Item avec :
    /// - id : "radiofrance:{station_slug}:stream"
    /// - parent_id : "radiofrance:{station_slug}"
    /// - title, artist, album, genre selon le mapping API → UPnP
    /// - resource unique avec l'URL du stream HiFi
    /// - album_art pointant vers la cover cachée
    pub async fn from_live_metadata(
        station: Station,
        metadata: LiveResponse,
        cache_manager: &SourceCacheManager,
    ) -> Result<Self>;
    
    /// Mettre à jour les métadonnées volatiles de l'item
    /// 
    /// Met à jour uniquement les champs volatiles :
    /// - title, artist, album (depuis nouvelles métadonnées)
    /// - album_art / album_art_pk (si nouvelle cover)
    /// 
    /// L'URL du stream (resource.url) ne change JAMAIS.
    pub async fn update_metadata(
        &mut self,
        metadata: &LiveResponse,
        cache_manager: &SourceCacheManager,
    ) -> Result<()>;
    
    /// Construire un pmodidl::Item depuis les métadonnées live
    async fn build_item_from_metadata(
        station: &Station,
        metadata: &LiveResponse,
        cache_manager: &SourceCacheManager,
    ) -> Result<Item>;
}

impl StationGroups {
    /// Organiser une liste de stations en groupes
    pub fn from_stations(stations: Vec<Station>) -> Self;
}
```

**Mapping API → UPnP (avec pmodidl::Item)** :

**Construction du pmodidl::Item** :

```rust
use pmodidl::{Item, Resource};

// Champs communs à tous les items
let item = Item {
    id: format!("radiofrance:{}:stream", station.slug),
    parent_id: format!("radiofrance:{}", station.slug),
    restricted: Some("1".to_string()),
    class: "object.item.audioItem.audioBroadcast".to_string(),
    
    // Métadonnées volatiles (varient selon radio parlée/musicale)
    title: ...,
    creator: ...,
    artist: ...,
    album: ...,
    genre: ...,
    
    // Cover
    album_art: Some(cover_url),
    album_art_pk: Some(cover_pk),
    
    // Resource unique (stream)
    resources: vec![Resource {
        protocol_info: "http-get:*:audio/aac:*".to_string(),
        bits_per_sample: None,
        sample_frequency: Some("48000".to_string()),
        nr_audio_channels: Some("2".to_string()),
        duration: None,  // Pas de durée pour un stream live
        url: stream_url,
    }],
    
    // Pas de descriptions pour les streams Radio France
    descriptions: vec![],
    
    // Autres champs optionnels
    date: None,
    original_track_number: None,
};
```

Pour **radios parlées** (France Culture, France Inter, France Info) :
- `title` = `now.firstLine.title` + " • " + `now.secondLine.title`
- `creator` = `now.producer`
- `artist` = `now.producer`
- `album` = `now.firstLine.title` (nom de l'émission)
- `genre` = Some("Talk Radio")
- `class` = "object.item.audioItem.audioBroadcast"

Pour **radios musicales** (FIP, France Musique) :
- Si `now.song` présent :
  - `title` = `now.firstLine.title` (titre du morceau)
  - `creator` = `now.song.interpreters.join(", ")`
  - `artist` = `now.song.interpreters.join(", ")`
  - `album` = `now.song.release.title`
  - `genre` = Some("Music")
  - `class` = "object.item.audioItem.musicTrack"
- Sinon (talk segment) : utiliser mapping radio parlée

**Cache des covers** :
- Extraire UUID depuis `now.visualBackground.src` ou image du `now.song`
- Construire URL haute résolution : `ImageSize::XLarge` ou `ImageSize::Large`
- Cacher via `cache_manager.cache_cover(url)`
- Stocker PK : `RADIOFRANCE:{uuid}`
- Dans l'item :
  - `album_art` = URL publique de la cover (via serveur HTTP)
  - `album_art_pk` = PK du cache (pour retrouver le fichier)

**Resource (URL du stream)** :
- Priorité : AAC 192 kbps > HLS > AAC 128 kbps > MP3 128 kbps
- Utiliser `metadata.now.media.best_hifi_stream()`
- `protocol_info` :
  - AAC : `"http-get:*:audio/aac:*"`
  - HLS : `"http-get:*:application/vnd.apple.mpegurl:*"`
  - MP3 : `"http-get:*:audio/mpeg:*"`
- `sample_frequency` = `"48000"` pour AAC, None pour HLS
- `nr_audio_channels` = `"2"` pour AAC, None pour HLS
- `duration` = None (stream infini)
- `url` = URL complète du stream

#### 3. `pmoradiofrance/src/config_ext.rs` (MODIFIER/COMPLÉTER)

Étendre le trait `RadioFranceConfigExt` avec toutes les méthodes nécessaires.

**Constantes** :
```rust
const DEFAULT_RADIOFRANCE_BASE_URL: &str = "https://www.radiofrance.fr";
const DEFAULT_RADIOFRANCE_TIMEOUT_SECS: u64 = 30;
const DEFAULT_RADIOFRANCE_CACHE_TTL_DAYS: u64 = 7;
```

**Trait complet** :

```rust
pub trait RadioFranceConfigExt {
    // Activation
    fn get_radiofrance_enabled(&self) -> Result<bool>;
    fn set_radiofrance_enabled(&self, enabled: bool) -> Result<()>;
    
    // Configuration HTTP
    fn get_radiofrance_base_url(&self) -> Result<String>;
    fn set_radiofrance_base_url(&self, url: String) -> Result<()>;
    fn get_radiofrance_timeout_secs(&self) -> Result<u64>;
    fn set_radiofrance_timeout_secs(&self, secs: u64) -> Result<()>;
    
    // Cache des stations
    fn get_radiofrance_stations_cache(&self) -> Result<Option<CachedStationList>>;
    fn set_radiofrance_stations_cache(&self, cache: &CachedStationList) -> Result<()>;
    fn clear_radiofrance_stations_cache(&self) -> Result<()>;
    fn get_radiofrance_cache_ttl_days(&self) -> Result<u64>;
    fn set_radiofrance_cache_ttl_days(&self, days: u64) -> Result<()>;
    
    // Factory method
    fn create_radiofrance_client(&self) -> Result<RadioFranceStatefulClient>;
}
```

**Implémentation** :

Chemins dans la config :
- `["sources", "radiofrance", "enabled"]` - bool
- `["sources", "radiofrance", "base_url"]` - string
- `["sources", "radiofrance", "timeout_secs"]` - u64
- `["sources", "radiofrance", "cache_ttl_days"]` - u64
- `["sources", "radiofrance", "stations_cache"]` - serde_yaml::Value (structure `CachedStationList`)

**Auto-persistence** :
- `get_radiofrance_enabled()` : Persister `true` par défaut si absent
- `get_radiofrance_base_url()` : Persister `DEFAULT_RADIOFRANCE_BASE_URL` si absent
- `get_radiofrance_timeout_secs()` : Persister `DEFAULT_RADIOFRANCE_TIMEOUT_SECS` si absent
- `get_radiofrance_cache_ttl_days()` : Persister `DEFAULT_RADIOFRANCE_CACHE_TTL_DAYS` si absent

**Factory method** :
```rust
fn create_radiofrance_client(&self) -> Result<RadioFranceStatefulClient> {
    let base_url = self.get_radiofrance_base_url()?;
    let timeout_secs = self.get_radiofrance_timeout_secs()?;
    
    let http_client = RadioFranceClient::builder()
        .base_url(base_url)
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .await?;
    
    let config = get_config();
    let cache_manager = SourceCacheManager::from_registry("radiofrance".to_string())?;
    
    Ok(RadioFranceStatefulClient::with_client_and_config(
        http_client,
        config,
        cache_manager,
    ))
}
```

#### 4. `pmoradiofrance/src/models.rs` (MODIFIER)

Ajouter la structure `CachedStationList` si pas déjà présente (elle est définie dans le Round 3) :

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedStationList {
    pub stations: Vec<Station>,
    pub last_updated: u64,
    pub version: u32,
}

impl CachedStationList {
    pub const CURRENT_VERSION: u32 = 1;
    pub const DEFAULT_TTL_SECS: u64 = 7 * 24 * 3600;
    
    pub fn new(stations: Vec<Station>) -> Self;
    pub fn is_valid(&self, ttl_secs: u64) -> bool;
    pub fn is_valid_default(&self) -> bool;
    pub fn age_secs(&self) -> u64;
}
```

#### 5. `pmoradiofrance/src/lib.rs` (MODIFIER)

Ajouter les exports :

```rust
pub mod stateful_client;
pub mod playlist;

pub use stateful_client::RadioFranceStatefulClient;
pub use playlist::{StationGroups, StationGroup, StationPlaylist};

// Ré-exporter pmodidl::Item pour faciliter l'utilisation
pub use pmodidl::Item as RadioItem;
```

#### 6. `pmoradiofrance/Cargo.toml` (MODIFIER)

Vérifier que les dépendances sont présentes :

```toml
[dependencies]
# ... dépendances existantes du Round 3 ...

# Nouvelles pour Round 4
pmoconfig = { path = "../pmoconfig" }
pmocovers = { path = "../pmocovers" }
pmosource = { path = "../pmosource" }  # Pour SourceCacheManager
pmodidl = { path = "../pmodidl" }      # Pour Item et Resource

[features]
default = ["pmoconfig"]
pmoconfig = ["dep:pmoconfig"]
cache = ["dep:pmocovers", "pmosource/server"]
server = ["pmoconfig", "cache"]
```

### Règles métier importantes

1. **Renommage France Bleu → ICI** :
   - Slug conservé : `francebleu`, `francebleu_alsace`, etc.
   - Label affiché : "ICI", "ICI Alsace", etc.
   - Implémenter dans `Station::display_name()` ou helper similaire

2. **Organisation des stations** :
   - Standalone : France Culture, France Inter, France Info, Mouv'
   - Avec webradios : FIP (9 webradios), France Musique (5+ webradios)
   - Locales : ~40 radios ICI (France Bleu)

3. **Playlists volatiles** :
   - Un seul item par playlist (le stream)
   - URL du stream ne change JAMAIS
   - Métadonnées changent toutes les 2-5 minutes
   - Cover cachée dans `pmocovers`, PAS dans `pmoaudiocache`

4. **Stream HiFi** :
   - Toujours présenter le meilleur stream disponible
   - Ordre : AAC 192kbps > HLS > AAC 128kbps > MP3 128kbps

5. **Cache des stations** :
   - TTL : 7 jours par défaut (configurable)
   - Stockage : YAML dans pmoconfig
   - Versionnement : Invalider si version change

6. **Cache des métadonnées live** :
   - TTL : Dynamique (champ `delayToRefresh` de l'API)
   - Stockage : Mémoire (HashMap)
   - Par station

### Tests à ajouter

**Tests unitaires** (`stateful_client.rs`) :
- Validation du cache (TTL, version)
- Organisation des stations en groupes
- Mapping métadonnées API → UPnP

**Tests d'intégration** (avec `#[ignore]`) :
- Découverte et cache des stations
- Récupération métadonnées live avec cache
- Construction de playlists complètes
- Mise à jour métadonnées volatiles

### Résultat attendu

À la fin du Round 4bis, on doit avoir :

1. ✅ Client stateful fonctionnel avec cache intelligent
2. ✅ Structures de playlists UPnP complètes
3. ✅ Extension pmoconfig complète
4. ✅ Organisation hiérarchique des stations
5. ✅ Mapping API → UPnP pour radios parlées et musicales
6. ✅ Cache des covers via pmocovers
7. ✅ Tests unitaires et d'intégration

Le Round 5 pourra alors implémenter la `MusicSource` qui utilisera ce client stateful.

---

## Round 5 : Implémentation MusicSource et Intégration Serveur

### Objectif

Implémenter le trait `MusicSource` pour l'intégration UPnP et ajouter les routes serveur REST pour l'accès aux stations Radio France.

### Fichiers à créer

#### 1. `pmoradiofrance/src/source.rs` (NOUVEAU)

Implémentation du trait `MusicSource` pour Radio France.

```rust
use async_trait::async_trait;
use pmosource::{MusicSource, SourceMetadata, SourceCapabilities};
use crate::{RadioFranceStatefulClient, StationGroups, StationPlaylist};
use pmoconfig::Config;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

pub struct RadioFranceSource {
    /// Client stateful avec cache automatique
    client: RadioFranceStatefulClient,
    /// Cache des playlists par station (métadonnées volatiles)
    playlists: Arc<RwLock<HashMap<String, StationPlaylist>>>,
    /// Handles des tâches de rafraîchissement
    refresh_handles: Arc<RwLock<HashMap<String, JoinHandle<()>>>>,
}

impl RadioFranceSource {
    /// Créer une nouvelle source Radio France
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        let client = RadioFranceStatefulClient::new(config).await?;
        
        Ok(Self {
            client,
            playlists: Arc::new(RwLock::new(HashMap::new())),
            refresh_handles: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    /// Démarrer le rafraîchissement des métadonnées pour une station
    async fn start_metadata_refresh(&self, station_slug: &str) -> Result<()> {
        let mut handles = self.refresh_handles.write().await;
        
        // Si déjà en cours, ne rien faire
        if handles.contains_key(station_slug) {
            return Ok(());
        }
        
        let client = self.client.clone();
        let playlists = self.playlists.clone();
        let slug = station_slug.to_string();
        
        let handle = tokio::spawn(async move {
            loop {
                match client.get_live_metadata(&slug).await {
                    Ok(metadata) => {
                        let delay = Duration::from_millis(metadata.delay_to_refresh);
                        
                        // Mettre à jour la playlist
                        if let Ok(mut pls) = playlists.write() {
                            if let Some(playlist) = pls.get_mut(&slug) {
                                let _ = playlist.update_metadata(&metadata, None, None);
                            }
                        }
                        
                        tokio::time::sleep(delay).await;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to refresh metadata for {}: {}", slug, e);
                        tokio::time::sleep(Duration::from_secs(60)).await;
                    }
                }
            }
        });
        
        handles.insert(station_slug.to_string(), handle);
        Ok(())
    }
    
    /// Arrêter le rafraîchissement pour une station
    async fn stop_metadata_refresh(&self, station_slug: &str) {
        let mut handles = self.refresh_handles.write().await;
        if let Some(handle) = handles.remove(station_slug) {
            handle.abort();
        }
    }
    
    /// Construire l'arborescence UPnP dynamiquement depuis StationGroups
    async fn build_container_tree(&self) -> Result<Container> {
        let stations = self.client.get_stations().await?;
        let groups = StationGroups::from_stations(stations);
        
        let mut children = Vec::new();
        
        // 1. Stations standalone → Items directs (streamables)
        for station in &groups.standalone {
            children.push(ContainerChild::Item(self.build_station_item(station).await?));
        }
        
        // 2. Stations avec webradios → Containers
        for group in &groups.with_webradios {
            children.push(ContainerChild::Container(
                self.build_station_container(group).await?
            ));
        }
        
        // 3. Radios ICI → Container unique "Radios ICI"
        if !groups.local_radios.is_empty() {
            children.push(ContainerChild::Container(
                self.build_ici_container(&groups.local_radios).await?
            ));
        }
        
        Ok(Container {
            id: "radiofrance:root".to_string(),
            parent_id: "-1".to_string(),
            title: "Radio France".to_string(),
            class: "object.container".to_string(),
            children,
        })
    }
    
    /// Construire un Container pour une station avec webradios
    async fn build_station_container(&self, group: &StationGroup) -> Result<Container> {
        let mut items = vec![
            self.build_station_item(&group.main).await?  // Station principale en premier
        ];
        
        // Ajouter les webradios
        for webradio in &group.webradios {
            items.push(self.build_station_item(webradio).await?);
        }
        
        Ok(Container {
            id: format!("radiofrance:group:{}", group.main.slug),
            parent_id: "radiofrance:root".to_string(),
            title: group.main.name.clone(),
            class: "object.container".to_string(),
            children: items.into_iter().map(ContainerChild::Item).collect(),
        })
    }
    
    /// Construire le Container des radios ICI
    async fn build_ici_container(&self, local_radios: &[Station]) -> Result<Container> {
        let mut items = Vec::new();
        
        for station in local_radios {
            items.push(self.build_station_item(station).await?);
        }
        
        Ok(Container {
            id: "radiofrance:ici".to_string(),
            parent_id: "radiofrance:root".to_string(),
            title: "Radios ICI".to_string(),
            class: "object.container".to_string(),
            children: items.into_iter().map(ContainerChild::Item).collect(),
        })
    }
    
    /// Construire un Item UPnP pour une station
    async fn build_station_item(&self, station: &Station) -> Result<Item> {
        // Récupérer ou créer la playlist pour cette station
        let mut playlists = self.playlists.write().await;
        
        let playlist = if let Some(existing) = playlists.get(&station.slug) {
            existing.clone()
        } else {
            // Créer la playlist avec métadonnées initiales
            let metadata = self.client.get_live_metadata(&station.slug).await?;
            let playlist = StationPlaylist::from_live_metadata_no_cache(
                station.clone(),
                &metadata
            )?;
            playlists.insert(station.slug.clone(), playlist.clone());
            
            // Démarrer le rafraîchissement automatique
            drop(playlists); // Libérer le lock avant d'appeler start_metadata_refresh
            self.start_metadata_refresh(&station.slug).await?;
            
            playlist
        };
        
        Ok(playlist.stream_item.clone())
    }
}

#[async_trait]
impl MusicSource for RadioFranceSource {
    fn source_id(&self) -> &str { 
        "radiofrance" 
    }
    
    fn display_name(&self) -> &str { 
        "Radio France" 
    }
    
    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            supports_search: false,     // Pas de recherche
            supports_playlists: false,   // Streams live uniquement
            supports_streaming: true,    // Flux audio HiFi
            is_live: true,              // Contenu live
        }
    }
    
    async fn get_root_container(&self) -> Result<Container> {
        self.build_container_tree().await
    }
    
    async fn browse_container(&self, container_id: &str) -> Result<Vec<ContainerChild>> {
        match container_id {
            "radiofrance:root" => {
                let container = self.build_container_tree().await?;
                Ok(container.children)
            }
            id if id.starts_with("radiofrance:group:") => {
                let slug = id.strip_prefix("radiofrance:group:").unwrap();
                let stations = self.client.get_stations().await?;
                let groups = StationGroups::from_stations(stations);
                
                if let Some(group) = groups.with_webradios.iter()
                    .find(|g| g.main.slug == slug) 
                {
                    let container = self.build_station_container(group).await?;
                    Ok(container.children)
                } else {
                    Err(Error::other("Container not found"))
                }
            }
            "radiofrance:ici" => {
                let stations = self.client.get_stations().await?;
                let groups = StationGroups::from_stations(stations);
                let container = self.build_ici_container(&groups.local_radios).await?;
                Ok(container.children)
            }
            _ => Err(Error::other("Unknown container"))
        }
    }
    
    async fn get_item(&self, item_id: &str) -> Result<Item> {
        // Format: radiofrance:{slug}:stream
        let slug = item_id
            .strip_prefix("radiofrance:")
            .and_then(|s| s.strip_suffix(":stream"))
            .ok_or_else(|| Error::other("Invalid item ID"))?;
        
        let playlists = self.playlists.read().await;
        playlists.get(slug)
            .map(|p| p.stream_item.clone())
            .ok_or_else(|| Error::other("Item not found"))
    }
    
    async fn refresh(&self) -> Result<()> {
        // Rafraîchir la liste des stations
        self.client.refresh_stations().await?;
        
        // Les métadonnées sont rafraîchies automatiquement par les tâches
        Ok(())
    }
}
```

**Principe de génération dynamique** :

1. **Récupération des stations** via `client.get_stations()` 
2. **Groupement automatique** via `StationGroups::from_stations()`
3. **Arborescence générée** selon la structure des données :
   - Station sans webradios → Item direct
   - Station avec webradios → Container (main + webradios)
   - Radios locales → Container "Radios ICI"

**Exemple d'arborescence générée** :

```
Radio France/                          (root container)
├── France Inter (item)                (standalone)
├── France Info (item)                 (standalone)
├── France Culture (item)              (standalone)
├── Mouv' (item)                       (standalone)
├── FIP/                               (container - has webradios)
│   ├── FIP (item)                     (main)
│   ├── FIP Rock (item)                (webradio)
│   ├── FIP Jazz (item)                (webradio)
│   └── ...
├── France Musique/                    (container - has webradios)
│   ├── France Musique (item)          (main)
│   ├── France Musique Classique (item)(webradio)
│   └── ...
└── Radios ICI/                        (container)
    ├── ICI Alsace (item)
    ├── ICI Paris (item)
    └── ... (~44 radios)
```

#### 2. `pmoradiofrance/src/server_ext.rs` (NOUVEAU)

Extension pour `pmoserver` : routes REST et cache registry.

```rust
use axum::{
    Router, Json, 
    extract::{Path, State}, 
    response::{Response, IntoResponse},
    http::{StatusCode, HeaderMap},
    body::Body,
};
use pmoserver::Server;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use futures::StreamExt;

/// Tracking des connexions streaming actives
#[derive(Clone)]
struct ActiveStream {
    started_at: SystemTime,
    last_activity: Arc<RwLock<SystemTime>>,
}

/// État partagé pour le serveur Radio France
struct RadioFranceServerState {
    client: Arc<RadioFranceStatefulClient>,
    active_streams: Arc<RwLock<HashMap<String, Vec<ActiveStream>>>>,
}

pub trait RadioFranceServerExt {
    /// Initialiser les routes Radio France
    async fn init_radiofrance_routes(&mut self) -> Result<()>;
    
    /// Enregistrer le cache registry pour les covers
    fn register_radiofrance_cache(&mut self) -> Result<()>;
}

impl RadioFranceServerExt for Server {
    async fn init_radiofrance_routes(&mut self) -> Result<()> {
        let client = Arc::new(RadioFranceStatefulClient::new(self.config().clone()).await?);
        let state = Arc::new(RadioFranceServerState {
            client,
            active_streams: Arc::new(RwLock::new(HashMap::new())),
        });
        
        let router = Router::new()
            .route("/radiofrance/stations", get(get_stations))
            .route("/radiofrance/:slug/metadata", get(get_metadata))
            .route("/radiofrance/:slug/stream", get(proxy_stream))
            .with_state(state);
        
        self.add_router("/api", router)
    }
    
    fn register_radiofrance_cache(&mut self) -> Result<()> {
        // Le cache de covers est déjà partagé via le cache registry global
        // Les playlists utilisent automatiquement pmocovers avec collection "radiofrance"
        Ok(())
    }
}

// === Route Handlers ===

/// GET /api/radiofrance/stations
/// Retourne la liste groupée des stations
async fn get_stations(
    State(state): State<Arc<RadioFranceServerState>>
) -> Result<Json<StationGroups>, (StatusCode, String)> {
    let stations = state.client.get_stations().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let groups = StationGroups::from_stations(stations);
    Ok(Json(groups))
}

/// GET /api/radiofrance/{slug}/metadata
/// Retourne les métadonnées live pour une station (avec cache)
async fn get_metadata(
    Path(slug): Path<String>,
    State(state): State<Arc<RadioFranceServerState>>
) -> Result<Json<LiveResponse>, (StatusCode, String)> {
    let metadata = state.client.get_live_metadata(&slug).await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    
    Ok(Json(metadata))
}

/// GET /api/radiofrance/{slug}/stream
/// Proxie le flux AAC de Radio France (passthrough sans transcodage)
/// Permet le tracking des connexions actives et le rafraîchissement des métadonnées
async fn proxy_stream(
    Path(slug): Path<String>,
    State(state): State<Arc<RadioFranceServerState>>
) -> Result<Response, (StatusCode, String)> {
    // 1. Récupérer l'URL du stream HiFi
    let stream_url = state.client.get_stream_url(&slug).await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    
    // 2. Démarrer le stream source (reqwest)
    let response = reqwest::get(&stream_url).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed to connect to Radio France: {}", e)))?;
    
    // 3. Enregistrer la connexion active
    let active_stream = ActiveStream {
        started_at: SystemTime::now(),
        last_activity: Arc::new(RwLock::new(SystemTime::now())),
    };
    
    {
        let mut streams = state.active_streams.write().await;
        streams.entry(slug.clone())
            .or_insert_with(Vec::new)
            .push(active_stream.clone());
    }
    
    // 4. Démarrer le rafraîchissement des métadonnées pour cette station
    let refresh_state = state.clone();
    let refresh_slug = slug.clone();
    tokio::spawn(async move {
        metadata_refresh_task(refresh_slug, refresh_state).await;
    });
    
    // 5. Créer le stream proxy avec tracking
    let last_activity = active_stream.last_activity.clone();
    let byte_stream = response.bytes_stream().map(move |chunk| {
        // Mettre à jour l'activité à chaque chunk
        if let Ok(ref _data) = chunk {
            if let Ok(mut activity) = last_activity.try_write() {
                *activity = SystemTime::now();
            }
        }
        chunk.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    });
    
    // 6. Construire la réponse HTTP avec headers appropriés
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "audio/aac".parse().unwrap());
    headers.insert("Cache-Control", "no-cache".parse().unwrap());
    headers.insert("Transfer-Encoding", "chunked".parse().unwrap());
    
    Ok((headers, Body::from_stream(byte_stream)).into_response())
}

/// Tâche de rafraîchissement des métadonnées pour une station active
/// S'arrête automatiquement quand toutes les connexions sont fermées
async fn metadata_refresh_task(slug: String, state: Arc<RadioFranceServerState>) {
    tracing::info!("Starting metadata refresh for station: {}", slug);
    
    loop {
        // Vérifier s'il y a encore des connexions actives
        let has_active_connections = {
            let mut streams = state.active_streams.write().await;
            
            // Nettoyer les connexions inactives (>30s sans activité)
            if let Some(connections) = streams.get_mut(&slug) {
                connections.retain(|stream| {
                    if let Ok(last) = stream.last_activity.try_read() {
                        last.elapsed().unwrap_or(Duration::MAX) < Duration::from_secs(30)
                    } else {
                        true // Garder si on ne peut pas vérifier
                    }
                });
                
                !connections.is_empty()
            } else {
                false
            }
        };
        
        if !has_active_connections {
            tracing::info!("No active connections for {}, stopping metadata refresh", slug);
            break;
        }
        
        // Rafraîchir les métadonnées
        match state.client.get_live_metadata(&slug).await {
            Ok(metadata) => {
                let delay = Duration::from_millis(metadata.delay_to_refresh);
                tracing::debug!("Refreshed metadata for {}, next refresh in {:?}", slug, delay);
                tokio::time::sleep(delay).await;
            }
            Err(e) => {
                tracing::warn!("Failed to refresh metadata for {}: {}", slug, e);
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
    }
    
    // Nettoyer l'entrée de la HashMap quand il n'y a plus de connexions
    state.active_streams.write().await.remove(&slug);
}
```

**Routes API** :

| Route | Méthode | Description | Réponse | Cache |
|-------|---------|-------------|---------|-------|
| `/api/radiofrance/stations` | GET | Liste groupée des stations | `StationGroups` JSON | 7 jours |
| `/api/radiofrance/{slug}/metadata` | GET | Métadonnées live | `LiveResponse` JSON | `delayToRefresh` |
| `/api/radiofrance/{slug}/stream` | GET | Proxy streaming AAC (passthrough) | Stream audio/aac | - |
| `/covers/{pk}` | GET | Cover depuis cache | Image binaire | Persistant |

**Détails du proxy streaming** :

Le proxy ne fait **aucun transcodage** - il forward les bytes AAC tels quels depuis Radio France. Ses fonctions :

1. **Tracking des connexions** : Maintient un registre des stations en cours d'écoute
2. **Mise à jour d'activité** : Enregistre un timestamp à chaque chunk reçu
3. **Déclenchement du refresh** : Lance automatiquement le rafraîchissement des métadonnées
4. **Nettoyage automatique** : Arrête le refresh quand toutes les connexions sont inactives (>30s)

Pourquoi un proxy plutôt qu'une redirection 302 ?
- ✅ Tracking précis des stations écoutées
- ✅ Rafraîchissement intelligent basé sur l'usage réel
- ✅ Pas de problème de décodage AAC streaming (contrainte technique actuelle)
- ✅ Consommation CPU minimale (juste forward de bytes)
- ❌ Bande passante serveur utilisée (mais LAN domestique → non critique)

### Fichiers à modifier

| Fichier | Modification |
|---------|--------------|
| `pmoradiofrance/src/lib.rs` | Ajout `#[cfg(feature = "server")] pub mod source;` et `pub mod server_ext;` |
| `pmoradiofrance/Cargo.toml` | Feature `server` inclut `dep:axum` dans les dépendances |

### Règles métier importantes

1. **Génération dynamique de l'arborescence** :
   - Utiliser `StationGroups::from_stations()` pour grouper
   - Pas de hardcoding des containers
   - La structure suit les données de l'API
   
2. **Rafraîchissement des métadonnées** :
   - Une tâche tokio par station streamée activement
   - Respecter `delayToRefresh` de l'API (2-5 minutes typiquement)
   - Arrêter automatiquement quand toutes les connexions proxy sont fermées
   - Nettoyage périodique des connexions inactives (>30s sans chunk)
   
3. **Gestion du cache de covers** :
   - Collection `"radiofrance"` dans `pmocovers`
   - URLs servies via `/covers/{pk}` (cache registry global)
   - Pas besoin d'enregistrement spécial (déjà partagé)
   
4. **Proxy streaming** :
   - Route `/radiofrance/{slug}/stream` proxie le flux AAC (passthrough)
   - **Aucun transcodage** : forward bytes AAC tels quels
   - Tracking des connexions actives via `ActiveStream`
   - Headers appropriés : `Content-Type: audio/aac`, `Transfer-Encoding: chunked`
   - URL source = flux HiFi AAC 192 kbps (ou HLS en fallback)

5. **Items vs Containers** :
   - Station **sans** webradios → Item direct (streamable)
   - Station **avec** webradios → Container contenant main + webradios
   - Radios ICI → Container unique regroupant toutes les radios locales
   
6. **Protocol Info UPnP** :
   - AAC : `"http-get:*:audio/aac:*"`
   - HLS : `"http-get:*:application/vnd.apple.mpegurl:*"`
   - Sample rate : `48000` Hz (AAC), None (HLS)
   - Channels : `2` (stéréo)

### Tests à ajouter

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    // === Tests unitaires source.rs ===
    
    #[tokio::test]
    async fn test_source_creation() {
        let config = Arc::new(get_test_config());
        let source = RadioFranceSource::new(config).await;
        assert!(source.is_ok());
    }
    
    #[tokio::test]
    async fn test_build_container_tree_structure() {
        let source = create_test_source().await;
        let tree = source.build_container_tree().await.unwrap();
        
        assert_eq!(tree.id, "radiofrance:root");
        assert!(!tree.children.is_empty());
        
        // Vérifier qu'on a bien des items et des containers
        let has_items = tree.children.iter()
            .any(|c| matches!(c, ContainerChild::Item(_)));
        let has_containers = tree.children.iter()
            .any(|c| matches!(c, ContainerChild::Container(_)));
            
        assert!(has_items, "Should have standalone station items");
        assert!(has_containers, "Should have station group containers");
    }
    
    #[tokio::test]
    async fn test_browse_station_group() {
        let source = create_test_source().await;
        
        // Browse le container FIP
        let children = source.browse_container("radiofrance:group:fip").await.unwrap();
        
        assert!(!children.is_empty());
        // FIP principal + webradios (rock, jazz, etc.)
        assert!(children.len() > 1);
    }
    
    #[tokio::test]
    async fn test_browse_ici_container() {
        let source = create_test_source().await;
        
        let children = source.browse_container("radiofrance:ici").await.unwrap();
        
        // Devrait avoir ~40+ radios locales
        assert!(children.len() >= 30);
    }
    
    #[tokio::test]
    async fn test_get_station_item() {
        let source = create_test_source().await;
        
        let item = source.get_item("radiofrance:franceculture:stream").await.unwrap();
        
        assert_eq!(item.id, "radiofrance:franceculture:stream");
        assert!(!item.resources.is_empty());
        assert!(!item.resources[0].url.is_empty());
    }
    
    #[tokio::test]
    async fn test_metadata_refresh_task() {
        let source = create_test_source().await;
        
        source.start_metadata_refresh("franceculture").await.unwrap();
        
        // Vérifier que la tâche tourne
        let handles = source.refresh_handles.read().await;
        assert!(handles.contains_key("franceculture"));
        
        // Arrêter
        drop(handles);
        source.stop_metadata_refresh("franceculture").await;
        
        let handles = source.refresh_handles.read().await;
        assert!(!handles.contains_key("franceculture"));
    }
    
    // === Tests intégration serveur ===
    
    #[tokio::test]
    #[ignore = "Integration test - requires server"]
    async fn test_api_get_stations() {
        let response = reqwest::get("http://localhost:8080/api/radiofrance/stations")
            .await.unwrap();
        
        assert_eq!(response.status(), 200);
        
        let groups: StationGroups = response.json().await.unwrap();
        assert!(!groups.standalone.is_empty());
        assert!(!groups.with_webradios.is_empty());
        assert!(!groups.local_radios.is_empty());
    }
    
    #[tokio::test]
    #[ignore = "Integration test - requires server"]
    async fn test_api_get_metadata() {
        let response = reqwest::get("http://localhost:8080/api/radiofrance/franceculture/metadata")
            .await.unwrap();
        
        assert_eq!(response.status(), 200);
        
        let metadata: LiveResponse = response.json().await.unwrap();
        assert_eq!(metadata.station_name, "franceculture");
        assert!(metadata.delay_to_refresh > 0);
    }
    
    #[tokio::test]
    #[ignore = "Integration test - requires server"]
    async fn test_api_stream_proxy() {
        let response = reqwest::get("http://localhost:8080/api/radiofrance/fip/stream")
            .await.unwrap();
        
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("Content-Type").unwrap(), "audio/aac");
        
        // Lire quelques chunks pour vérifier que le stream fonctionne
        let mut stream = response.bytes_stream();
        let mut chunks_received = 0;
        
        while let Some(chunk) = stream.next().await {
            assert!(chunk.is_ok());
            chunks_received += 1;
            
            if chunks_received >= 5 {
                break; // Suffisant pour tester
            }
        }
        
        assert!(chunks_received >= 5, "Should receive streaming chunks");
    }
    
    #[tokio::test]
    #[ignore = "Integration test - requires server"]
    async fn test_metadata_refresh_starts_on_stream() {
        // Démarrer un stream
        let mut stream = reqwest::get("http://localhost:8080/api/radiofrance/franceculture/stream")
            .await.unwrap()
            .bytes_stream();
        
        // Lire quelques chunks
        for _ in 0..3 {
            stream.next().await;
        }
        
        // Vérifier que les métadonnées sont rafraîchies
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        let metadata = reqwest::get("http://localhost:8080/api/radiofrance/franceculture/metadata")
            .await.unwrap()
            .json::<LiveResponse>()
            .await.unwrap();
        
        assert_eq!(metadata.station_name, "franceculture");
    }
}
```

### Optimisations

1. **Pool de connexions HTTP partagé** :
   - Le `RadioFranceStatefulClient` utilise déjà un `reqwest::Client` interne
   - Configuration pool : `pool_max_idle_per_host = 10`
   - Réutilisation des connexions TCP

2. **Préchargement intelligent** :
   - Au démarrage serveur, déclencher refresh pour stations populaires
   - Liste configurable : `["franceinter", "fip", "franceculture", "franceinfo"]`
   - Charge le cache avant première requête utilisateur

3. **Nettoyage automatique des connexions** :
   - Vérifier périodiquement les connexions inactives (>30s sans chunk)
   - Arrêter la tâche de refresh quand toutes les connexions sont fermées
   - HashMap `active_streams` nettoyée automatiquement

4. **Métriques** :
   - Compteur hit/miss cache stations
   - Compteur hit/miss cache métadonnées
   - Temps moyen de rafraîchissement par station
   - Nombre de connexions actives par station
   - Bande passante proxy totale

### Résultat attendu

À la fin du Round 5 :

1. ✅ Trait `MusicSource` implémenté
2. ✅ Arborescence UPnP générée dynamiquement depuis les données
3. ✅ Routes API REST complètes et fonctionnelles
4. ✅ Proxy streaming AAC avec tracking des connexions
5. ✅ Rafraîchissement automatique des métadonnées basé sur l'usage réel
6. ✅ Cache multi-niveaux opérationnel
7. ✅ Tests unitaires et d'intégration
8. ✅ ~70 stations Radio France accessibles via UPnP et API

Radio France sera alors **pleinement intégré** dans PMOMusic :
- ✅ Navigation UPnP hiérarchique sur contrôleurs compatibles
- ✅ API REST pour webapp/clients HTTP
- ✅ Streaming AAC passthrough (pas de transcodage pour l'instant)
- ✅ Tracking intelligent : rafraîchissement uniquement des stations écoutées
- ✅ Cache intelligent (stations 7j + métadonnées dynamique)
- ✅ Toutes les stations principales, webradios et radios ICI

**Note technique** : Le proxy AAC passthrough est un compromis pragmatique dû aux limitations actuelles du décodage AAC streaming dans `pmoflac`. Si cette capacité est ajoutée à l'avenir, le transcodage vers FLAC pourra être implémenté via une feature flag optionnelle.

## Round 6

Il faut maintenant activer cette nouvelle source.
Tu peux regarder dans les deux autres sources actuelles comment cette initialisation est réalisée 
- [@pmoqobuz](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoqobuz) 
- [@pmoparadise](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoparadise) 

Ainsi que dans la crate [@pmoupnp](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoupnp)
