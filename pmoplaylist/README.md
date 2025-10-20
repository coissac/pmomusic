# pmoplaylist

FIFO Audio Universelle pour MediaServer UPnP/OpenHome en Rust.

## Description

`pmoplaylist` fournit une abstraction de playlist/container audio avec :

- ✅ Gestion de FIFO audio avec capacité configurable
- ✅ Exposition d'objets DIDL-Lite via `pmodidl`
- ✅ Support `update_id` et `last_change` pour signaler les modifications
- ✅ Image par défaut intégrée pour le container racine (WebP)
- ✅ Thread-safe avec `tokio` et `Arc<RwLock>`
- ✅ API asynchrone compatible avec les MediaServers UPnP

## Installation

Ajoutez cette crate à votre `Cargo.toml` :

```toml
[dependencies]
pmoplaylist = { path = "../pmoplaylist" }
tokio = { version = "1.42.0", features = ["full"] }
```

## Utilisation de base

### Créer une playlist FIFO

```rust
use pmoplaylist::{FifoPlaylist, Track};

#[tokio::main]
async fn main() {
    // Créer une FIFO avec capacité de 10 tracks
    let playlist = FifoPlaylist::new(
        "radio-1".to_string(),
        "Ma Radio Préférée".to_string(),
        10,
        pmoplaylist::DEFAULT_IMAGE,
    );

    // Vérifier l'état initial
    assert_eq!(playlist.len().await, 0);
    assert!(playlist.is_empty().await);
}
```

### Ajouter des tracks

```rust
use pmoplaylist::{FifoPlaylist, Track};

#[tokio::main]
async fn main() {
    let playlist = FifoPlaylist::new(
        "my-playlist".to_string(),
        "My Playlist".to_string(),
        50,
        pmoplaylist::DEFAULT_IMAGE,
    );

    // Méthode simple
    let track1 = Track::new(
        "track-1",
        "Bohemian Rhapsody",
        "http://example.com/queen/bohemian.flac"
    );
    playlist.append_track(track1).await;

    // Avec builder pattern pour métadonnées complètes
    let track2 = Track::new("track-2", "Stairway to Heaven", "http://example.com/zeppelin/stairway.mp3")
        .with_artist("Led Zeppelin")
        .with_album("Led Zeppelin IV")
        .with_duration(482)
        .with_image("http://example.com/covers/lz4.jpg");

    playlist.append_track(track2).await;

    println!("Nombre de tracks: {}", playlist.len().await);
}
```

### Gestion FIFO automatique

La FIFO supprime automatiquement les tracks les plus anciens quand la capacité est atteinte :

```rust
use pmoplaylist::{FifoPlaylist, Track};

#[tokio::main]
async fn main() {
    // Créer une FIFO avec capacité de 3 tracks seulement
    let playlist = FifoPlaylist::new(
        "small-fifo".to_string(),
        "Petite FIFO".to_string(),
        3,
        pmoplaylist::DEFAULT_IMAGE,
    );

    // Ajouter 5 tracks
    for i in 0..5 {
        playlist.append_track(Track::new(
            format!("track-{}", i),
            format!("Song {}", i),
            format!("http://example.com/{}.mp3", i)
        )).await;
    }

    // Seuls les 3 derniers restent (tracks 2, 3, 4)
    assert_eq!(playlist.len().await, 3);

    let items = playlist.get_items(0, 10).await;
    assert_eq!(items[0].id, "track-2");
    assert_eq!(items[2].id, "track-4");
}
```

### Navigation et pagination

```rust
use pmoplaylist::{FifoPlaylist, Track};

#[tokio::main]
async fn main() {
    let playlist = FifoPlaylist::new(
        "big-playlist".to_string(),
        "Grande Playlist".to_string(),
        100,
        pmoplaylist::DEFAULT_IMAGE,
    );

    // Ajouter 50 tracks
    for i in 0..50 {
        playlist.append_track(Track::new(
            format!("track-{}", i),
            format!("Song {}", i),
            format!("http://example.com/{}.mp3", i)
        )).await;
    }

    // Récupérer les tracks 10 à 19 (navigation paginée)
    let page = playlist.get_items(10, 10).await;
    assert_eq!(page.len(), 10);
    assert_eq!(page[0].id, "track-10");
}
```

### Supprimer des tracks

```rust
use pmoplaylist::{FifoPlaylist, Track};

#[tokio::main]
async fn main() {
    let playlist = FifoPlaylist::new(
        "playlist-1".to_string(),
        "My Playlist".to_string(),
        10,
        pmoplaylist::DEFAULT_IMAGE,
    );

    playlist.append_track(Track::new("track-1", "Song 1", "http://example.com/1.mp3")).await;
    playlist.append_track(Track::new("track-2", "Song 2", "http://example.com/2.mp3")).await;

    // Supprimer le plus ancien (FIFO)
    let removed = playlist.remove_oldest().await;
    assert_eq!(removed.unwrap().id, "track-1");

    // Supprimer par ID
    playlist.remove_by_id("track-2").await;

    // Vider complètement
    playlist.clear().await;
    assert!(playlist.is_empty().await);
}
```

### Détection de changements (update_id)

L'`update_id` est incrémenté à chaque modification de la playlist :

```rust
use pmoplaylist::{FifoPlaylist, Track};

#[tokio::main]
async fn main() {
    let playlist = FifoPlaylist::new(
        "watched-playlist".to_string(),
        "Watched Playlist".to_string(),
        10,
        pmoplaylist::DEFAULT_IMAGE,
    );

    let initial_id = playlist.update_id().await;
    assert_eq!(initial_id, 0);

    // Chaque opération incrémente l'update_id
    playlist.append_track(Track::new("track-1", "Song", "http://example.com/1.mp3")).await;
    assert_eq!(playlist.update_id().await, 1);

    playlist.append_track(Track::new("track-2", "Song", "http://example.com/2.mp3")).await;
    assert_eq!(playlist.update_id().await, 2);

    playlist.remove_oldest().await;
    assert_eq!(playlist.update_id().await, 3);

    // Timestamp de dernière modification
    let last_change = playlist.last_change().await;
    println!("Dernière modification: {:?}", last_change);
}
```

## Intégration UPnP/DIDL-Lite

### Générer un Container DIDL-Lite

```rust
use pmoplaylist::FifoPlaylist;

#[tokio::main]
async fn main() {
    let playlist = FifoPlaylist::new(
        "radio-paradise".to_string(),
        "Radio Paradise".to_string(),
        20,
        pmoplaylist::DEFAULT_IMAGE,
    );

    // Générer le container pour ContentDirectory
    let container = playlist.as_container().await;

    println!("Container ID: {}", container.id);
    println!("Title: {}", container.title);
    println!("Child count: {:?}", container.child_count);
    println!("Class: {}", container.class); // "object.container.playlistContainer"
}
```

### Générer des Items DIDL-Lite

```rust
use pmoplaylist::{FifoPlaylist, Track};

#[tokio::main]
async fn main() {
    let playlist = FifoPlaylist::new(
        "radio-1".to_string(),
        "Ma Radio".to_string(),
        10,
        pmoplaylist::DEFAULT_IMAGE,
    );

    // Ajouter des tracks
    let track = Track::new("track-1", "Bohemian Rhapsody", "http://example.com/song.mp3")
        .with_artist("Queen")
        .with_album("A Night at the Opera")
        .with_duration(354);

    playlist.append_track(track).await;

    // Générer les items DIDL-Lite avec URL de l'image par défaut
    let items = playlist.as_objects(
        0,                                      // offset
        10,                                     // count
        Some("http://myserver/default.webp")    // URL pour l'image par défaut
    ).await;

    for item in items {
        println!("Item: {}", item.title);
        println!("  Artist: {:?}", item.artist);
        println!("  Album: {:?}", item.album);
        println!("  URI: {}", item.resources[0].url);
        println!("  Class: {}", item.class); // "object.item.audioItem.musicTrack"
    }
}
```

### Servir l'image par défaut

```rust
use pmoplaylist::FifoPlaylist;

#[tokio::main]
async fn main() {
    let playlist = FifoPlaylist::new(
        "radio-1".to_string(),
        "Ma Radio".to_string(),
        10,
        pmoplaylist::DEFAULT_IMAGE,
    );

    // Récupérer les bytes de l'image par défaut
    let image_bytes = playlist.default_image().await;

    // Peut être servi via un endpoint HTTP, par exemple avec Axum:
    // Response::builder()
    //     .status(200)
    //     .header("Content-Type", "image/webp")
    //     .body(image_bytes.to_vec())
}
```

## Cas d'usage

### Radio dynamique en streaming

```rust
use pmoplaylist::{FifoPlaylist, Track};

#[tokio::main]
async fn main() {
    // Radio avec historique limité à 20 tracks
    let radio = FifoPlaylist::new(
        "radio-paradise".to_string(),
        "Radio Paradise".to_string(),
        20,
        pmoplaylist::DEFAULT_IMAGE,
    );

    // Simuler l'ajout de tracks au fur et à mesure du streaming
    // Les anciens tracks sont automatiquement supprimés
    for i in 0..100 {
        let track = Track::new(
            format!("track-{}", i),
            format!("Now Playing: Song {}", i),
            format!("http://stream.radio.com/track/{}", i)
        );
        radio.append_track(track).await;

        // La radio conserve toujours les 20 derniers tracks
        assert!(radio.len().await <= 20);
    }
}
```

### Album statique

```rust
use pmoplaylist::{FifoPlaylist, Track};

#[tokio::main]
async fn main() {
    // Album avec tous les tracks
    let album = FifoPlaylist::new(
        "album-dsotm".to_string(),
        "The Dark Side of the Moon".to_string(),
        100,  // Capacité large pour un album complet
        pmoplaylist::DEFAULT_IMAGE,
    );

    // Ajouter tous les tracks de l'album
    let tracks = vec![
        ("1", "Speak to Me", 90),
        ("2", "Breathe", 163),
        ("3", "On the Run", 216),
        ("4", "Time", 413),
        ("5", "The Great Gig in the Sky", 283),
        ("6", "Money", 382),
        ("7", "Us and Them", 462),
        ("8", "Any Colour You Like", 205),
        ("9", "Brain Damage", 228),
        ("10", "Eclipse", 123),
    ];

    for (track_num, title, duration) in tracks {
        album.append_track(
            Track::new(
                format!("dsotm-{}", track_num),
                title,
                format!("http://library.local/floyd/dsotm/{}.flac", track_num)
            )
            .with_artist("Pink Floyd")
            .with_album("The Dark Side of the Moon")
            .with_duration(duration)
        ).await;
    }
}
```

## Thread Safety

`FifoPlaylist` est thread-safe et peut être cloné et partagé entre plusieurs threads/tasks :

```rust
use pmoplaylist::{FifoPlaylist, Track};
use tokio::task;

#[tokio::main]
async fn main() {
    let playlist = FifoPlaylist::new(
        "shared-playlist".to_string(),
        "Shared Playlist".to_string(),
        100,
        pmoplaylist::DEFAULT_IMAGE,
    );

    // Cloner pour partager entre threads
    let playlist_writer = playlist.clone();
    let playlist_reader = playlist.clone();

    // Thread d'écriture
    let writer = task::spawn(async move {
        for i in 0..10 {
            playlist_writer.append_track(Track::new(
                format!("track-{}", i),
                format!("Song {}", i),
                format!("http://example.com/{}.mp3", i)
            )).await;
        }
    });

    // Thread de lecture
    let reader = task::spawn(async move {
        loop {
            let len = playlist_reader.len().await;
            if len >= 10 {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        println!("Playlist complète!");
    });

    writer.await.unwrap();
    reader.await.unwrap();
}
```

## API complète

### `Track`

- `Track::new(id, title, uri)` - Crée un nouveau track
- `.with_artist(artist)` - Définit l'artiste
- `.with_album(album)` - Définit l'album
- `.with_duration(seconds)` - Définit la durée en secondes
- `.with_image(url)` - Définit l'URL de l'image

### `FifoPlaylist`

#### Création
- `FifoPlaylist::new(id, title, capacity, default_image)` - Crée une nouvelle playlist

#### Modification
- `.append_track(track)` - Ajoute un track (supprime le plus ancien si capacité atteinte)
- `.remove_oldest()` - Supprime le track le plus ancien
- `.remove_by_id(id)` - Supprime un track par son ID
- `.clear()` - Vide complètement la playlist

#### Lecture
- `.len()` - Nombre de tracks
- `.is_empty()` - Vérifie si vide
- `.get_items(offset, count)` - Récupère une portion des tracks
- `.id()` - Retourne l'ID de la playlist
- `.title()` - Retourne le titre de la playlist

#### Méta-données
- `.update_id()` - Retourne l'update_id actuel (incrémenté à chaque modification)
- `.last_change()` - Retourne le timestamp de dernière modification

#### DIDL-Lite
- `.as_container()` - Génère un Container DIDL-Lite (parent_id = "0")
- `.as_container_with_parent(parent_id)` - Génère un Container avec parent_id personnalisé
- `.as_objects(offset, count, default_image_url)` - Génère des Items DIDL-Lite
- `.default_image()` - Retourne les bytes de l'image par défaut

## Architecture

```
FifoPlaylist
├── Arc<RwLock<FifoPlaylistInner>>
│   ├── id: String
│   ├── title: String
│   ├── default_image: &'static [u8]
│   ├── capacity: usize
│   ├── queue: VecDeque<Track>
│   ├── update_id: u32
│   └── last_change: SystemTime
│
Track
├── id: String
├── title: String
├── artist: Option<String>
├── album: Option<String>
├── duration: Option<u32>
├── uri: String
└── image: Option<String>
```

## Dépendances

- `pmodidl` - Génération DIDL-Lite
- `tokio` - Runtime asynchrone et synchronisation
- `serde` - Sérialisation

## Tests

```bash
cargo test -p pmoplaylist
```

Tous les tests (unitaires et doctests) sont inclus et validés.

## Licence

Ce projet fait partie du workspace PMOMusic.
