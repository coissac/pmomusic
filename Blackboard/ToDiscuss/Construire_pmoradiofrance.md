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
