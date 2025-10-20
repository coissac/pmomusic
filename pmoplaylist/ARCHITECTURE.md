# Architecture de pmoplaylist

## Vue d'ensemble

`pmoplaylist` est une bibliothèque Rust qui fournit une abstraction de playlist FIFO (First-In-First-Out) thread-safe pour des MediaServers UPnP/OpenHome. Elle gère la logique de playlist pure sans aucune dépendance réseau ou protocole UPnP.

## Design Patterns

### 1. Arc + RwLock Pattern (Thread Safety)

```rust
pub struct FifoPlaylist {
    inner: Arc<RwLock<FifoPlaylistInner>>,
}
```

**Raison** : Permet le clonage léger de `FifoPlaylist` et le partage entre threads/tasks tout en garantissant un accès concurrent sécurisé.

**Avantages** :
- Clone peu coûteux (clone uniquement le `Arc`, pas les données)
- Accès concurrent : plusieurs lecteurs simultanés, un seul écrivain
- Compatible avec tokio et les runtimes asynchrones

**Exemple d'utilisation** :
```rust
let playlist = FifoPlaylist::new(...);
let p1 = playlist.clone(); // Pour un thread
let p2 = playlist.clone(); // Pour un autre thread
```

### 2. Builder Pattern pour Track

```rust
Track::new("id", "title", "uri")
    .with_artist("Artist")
    .with_album("Album")
    .with_duration(300)
    .with_image("url");
```

**Raison** : Facilite la création de tracks avec métadonnées optionnelles de manière fluide et lisible.

### 3. FIFO avec VecDeque

```rust
struct FifoPlaylistInner {
    queue: VecDeque<Track>,
    capacity: usize,
    // ...
}
```

**Raison** : `VecDeque` offre des opérations O(1) pour `push_back` et `pop_front`, parfait pour une FIFO.

**Gestion de la capacité** :
- Lors de `append_track()`, si `len >= capacity`, on appelle `pop_front()` automatiquement
- Garantit que la playlist ne dépasse jamais la capacité configurée

## Structures de données

### Track

```rust
pub struct Track {
    pub id: String,              // Identifiant unique
    pub title: String,           // Titre du morceau
    pub artist: Option<String>,  // Artiste
    pub album: Option<String>,   // Album
    pub duration: Option<u32>,   // Durée en secondes
    pub uri: String,             // URI du fichier/flux
    pub image: Option<String>,   // URL de la cover
}
```

**Sérialisation** : Implémente `Serialize` et `Deserialize` pour faciliter l'export JSON/autre.

### FifoPlaylistInner

```rust
struct FifoPlaylistInner {
    id: String,                    // ID unique de la playlist
    title: String,                 // Titre de la playlist
    default_image: &'static [u8],  // Image par défaut embarquée
    capacity: usize,               // Capacité max de la FIFO
    queue: VecDeque<Track>,        // Queue des tracks
    update_id: u32,                // Compteur de modifications
    last_change: SystemTime,       // Timestamp dernière modif
}
```

**update_id** :
- Incrémenté à chaque modification (append, remove, clear)
- Permet aux clients UPnP de détecter les changements
- Utilise `wrapping_add()` pour éviter les débordements

## Intégration DIDL-Lite

### Génération de Container

```rust
pub async fn as_container(&self) -> Container
```

**Produit** :
```xml
<container id="playlist-id" parentID="0" childCount="5">
  <dc:title>My Playlist</dc:title>
  <upnp:class>object.container.playlistContainer</upnp:class>
</container>
```

**Utilisation** : Pour exposer la playlist comme container dans le ContentDirectory UPnP.

### Génération d'Items

```rust
pub async fn as_objects(
    offset: usize,
    count: usize,
    default_image_url: Option<&str>
) -> Vec<Item>
```

**Produit** : Un vecteur d'objets `pmodidl::Item` représentant les tracks.

**Mapping Track → DIDL Item** :
- `track.id` → `item.id`
- `track.title` → `item.title`
- `track.artist` → `item.artist` et `item.creator`
- `track.album` → `item.album`
- `track.uri` → `resource.url`
- `track.duration` (secondes) → `resource.duration` (format "H:MM:SS")
- `track.image` ou `default_image_url` → `item.album_art`

**Classe UPnP** : Tous les items ont la classe `object.item.audioItem.musicTrack`.

## Gestion de l'image par défaut

### Intégration avec `include_bytes!`

```rust
pub const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/default.webp");
```

**Avantages** :
- L'image est compilée directement dans le binaire
- Pas de dépendance au système de fichiers à l'exécution
- Accès instantané et thread-safe

### Format WebP

**Raison du choix** :
- Format moderne et efficace
- Meilleure compression que JPEG/PNG
- Support alpha (transparence)
- Largement supporté par les navigateurs et clients modernes

**Spécifications** :
- Dimension : 300x300 pixels
- Format : WebP
- Qualité : 85
- Taille : ~9-10 KB

### Utilisation

```rust
let image_bytes = playlist.default_image().await;
// Servir via HTTP avec Content-Type: image/webp
```

## Concurrence et Thread Safety

### Scenario 1 : Lecture concurrente

```rust
// Thread 1
let len = playlist.len().await;

// Thread 2 (simultané)
let items = playlist.get_items(0, 10).await;
```

**Comportement** : Les deux opérations peuvent s'exécuter simultanément car `RwLock` permet plusieurs lecteurs.

### Scenario 2 : Écriture exclusive

```rust
// Thread 1
playlist.append_track(track1).await;

// Thread 2 (simultané)
playlist.append_track(track2).await;
```

**Comportement** : Les opérations sont sérialisées. Un seul thread écrit à la fois.

### Scenario 3 : Lecture pendant écriture

```rust
// Thread 1 : Écriture
playlist.append_track(track).await;

// Thread 2 : Lecture (simultané)
let len = playlist.len().await;
```

**Comportement** : La lecture attend que l'écriture se termine.

## Gestion de l'Update ID

### Algorithme

```rust
// À chaque modification
inner.update_id = inner.update_id.wrapping_add(1);
inner.last_change = SystemTime::now();
```

**Opérations qui incrémentent l'update_id** :
- `append_track()` → +1
- `remove_oldest()` → +1 (si un track est supprimé)
- `remove_by_id()` → +1 (si un track est trouvé et supprimé)
- `clear()` → +1 (si la playlist n'était pas vide)

**Opérations qui ne l'incrémentent PAS** :
- `get_items()` (lecture seule)
- `len()`, `is_empty()` (lecture seule)
- `as_container()`, `as_objects()` (lecture seule)

### Utilisation dans UPnP

Les clients UPnP peuvent :
1. Interroger l'`update_id` initial
2. Mémoriser cette valeur
3. Ré-interroger périodiquement
4. Si `update_id` a changé → rafraîchir l'affichage

## Cas d'usage

### 1. Radio en streaming

**Caractéristiques** :
- Capacité limitée (ex: 20 tracks)
- Ajouts fréquents de nouveaux tracks
- Les anciens tracks sont automatiquement supprimés

**Configuration recommandée** :
```rust
let radio = FifoPlaylist::new(
    "radio-paradise",
    "Radio Paradise",
    20,  // Historique limité à 20 tracks
    DEFAULT_IMAGE,
);
```

### 2. Album statique

**Caractéristiques** :
- Capacité large (ex: 100 tracks)
- Tous les tracks ajoutés une seule fois
- Pas de rotation automatique

**Configuration recommandée** :
```rust
let album = FifoPlaylist::new(
    "album-dsotm",
    "The Dark Side of the Moon",
    100,  // Capacité large pour tout l'album
    DEFAULT_IMAGE,
);
```

### 3. Playlist locale modifiable

**Caractéristiques** :
- Capacité moyenne (ex: 50 tracks)
- Ajouts et suppressions manuels
- Utilisation de `remove_by_id()` pour contrôle précis

**Configuration recommandée** :
```rust
let playlist = FifoPlaylist::new(
    "my-playlist",
    "My Favorites",
    50,
    DEFAULT_IMAGE,
);
```

## Intégration avec un MediaServer

### Architecture typique

```
┌─────────────────┐
│  UPnP Client    │
│  (Control Point)│
└────────┬────────┘
         │ HTTP/SOAP
         ▼
┌─────────────────────┐
│  MediaServer UPnP   │
│  ┌───────────────┐  │
│  │ ContentDirectory│  │
│  │    Service    │  │
│  └───────┬───────┘  │
│          │          │
│          ▼          │
│  ┌───────────────┐  │
│  │ pmoplaylist   │  │ ← Cette crate
│  │   (FIFO)      │  │
│  └───────────────┘  │
└─────────────────────┘
```

### Exemple d'endpoints

```rust
// GET /ContentDirectory/Browse?ObjectID=playlist-id
async fn browse_container(playlist: Arc<FifoPlaylist>) -> Response {
    let container = playlist.as_container().await;
    // Convertir en XML DIDL-Lite et retourner
}

// GET /ContentDirectory/Browse?ObjectID=playlist-id&StartingIndex=0&RequestedCount=10
async fn browse_items(
    playlist: Arc<FifoPlaylist>,
    offset: usize,
    count: usize
) -> Response {
    let items = playlist.as_objects(offset, count, Some(DEFAULT_IMAGE_URL)).await;
    // Convertir en XML DIDL-Lite et retourner
}

// GET /SystemUpdateID
async fn get_update_id(playlist: Arc<FifoPlaylist>) -> Response {
    let update_id = playlist.update_id().await;
    // Retourner l'update_id
}
```

## Tests

### Couverture

La crate inclut 11 tests unitaires + 8 doctests couvrant :

1. **Création et état initial**
   - `test_create_playlist`

2. **Ajout de tracks**
   - `test_append_track`
   - `test_fifo_capacity`

3. **Suppression de tracks**
   - `test_remove_oldest`
   - `test_remove_by_id`
   - `test_clear`

4. **Navigation**
   - `test_get_items_pagination`

5. **Génération DIDL-Lite**
   - `test_as_container`
   - `test_as_objects`

6. **Builder pattern**
   - `test_track_builder`

7. **Update ID**
   - `test_update_id_increments`

### Exécution

```bash
# Tests unitaires
cargo test -p pmoplaylist

# Tests avec doctests
cargo test -p pmoplaylist --doc

# Tous les tests
cargo test -p pmoplaylist --all-targets
```

## Exemples fournis

### 1. basic_usage.rs

Démontre :
- Création d'une playlist
- Ajout et suppression de tracks
- Comportement FIFO
- Génération DIDL-Lite
- Gestion de l'update_id

```bash
cargo run -p pmoplaylist --example basic_usage
```

### 2. radio_streaming.rs

Démontre :
- Utilisation multi-thread
- Simulation d'un flux radio continu
- Surveillance des changements via update_id
- Consultation de l'historique

```bash
cargo run -p pmoplaylist --example radio_streaming
```

### 3. http_server_integration.rs

Démontre :
- Intégration avec un serveur HTTP
- Endpoints REST simulés
- Partage de playlist avec `Arc`
- Serving de l'image par défaut

```bash
cargo run -p pmoplaylist --example http_server_integration
```

## Dépendances

### Runtime

- **pmodidl** (path = "../pmodidl")
  - Structures DIDL-Lite (Container, Item, Resource)
  - Nécessaire pour la génération d'objets UPnP

- **tokio** (1.42.0, features: sync, time, macros, rt, rt-multi-thread)
  - RwLock asynchrone pour thread safety
  - Runtime asynchrone pour les méthodes async

- **serde** (1.0.228, features: derive)
  - Sérialisation/désérialisation de Track
  - Support JSON/autres formats si nécessaire

### Build-time

- **include_bytes!** (macro std)
  - Intégration de l'image par défaut dans le binaire

## Performance

### Complexité algorithmique

- `append_track()` : O(1) amorti (VecDeque::push_back + potentiel pop_front)
- `remove_oldest()` : O(1) (VecDeque::pop_front)
- `remove_by_id()` : O(n) (recherche linéaire + VecDeque::remove)
- `get_items()` : O(k) où k = count (iteration + clone)
- `clear()` : O(n) (libération de tous les tracks)

### Allocation mémoire

- Chaque `Track` : ~100-200 bytes (selon la taille des strings)
- VecDeque overhead : ~24 bytes + capacity
- RwLock overhead : ~40 bytes
- Arc overhead : ~16 bytes

**Exemple** : Une playlist de 20 tracks ≈ 2-4 KB

### Lock contention

**Read-heavy workload** : Excellent (RwLock permet plusieurs lecteurs)

**Write-heavy workload** : Acceptable (les écritures sont généralement peu fréquentes pour une playlist)

**Recommandation** : Pour des milliers d'écritures/seconde, envisager un design lock-free ou sharding.

## Extensions futures possibles

### 1. Persistence

```rust
impl FifoPlaylist {
    pub async fn save_to_disk(&self, path: &Path) -> io::Result<()>;
    pub async fn load_from_disk(path: &Path) -> io::Result<Self>;
}
```

### 2. Événements et callbacks

```rust
pub enum PlaylistEvent {
    TrackAdded(Track),
    TrackRemoved(String),
    Cleared,
}

impl FifoPlaylist {
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<PlaylistEvent>;
}
```

### 3. Indexation et recherche

```rust
impl FifoPlaylist {
    pub async fn find_by_artist(&self, artist: &str) -> Vec<Track>;
    pub async fn find_by_title(&self, title: &str) -> Vec<Track>;
}
```

### 4. Statistiques

```rust
impl FifoPlaylist {
    pub async fn total_duration(&self) -> u32;
    pub async fn most_common_artist(&self) -> Option<String>;
}
```

## Licence

Ce projet fait partie du workspace PMOMusic.
