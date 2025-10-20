# pmoqobuz - Client Qobuz pour PMOMusic

Client Rust pour l'API Qobuz avec cache en mémoire, inspiré de l'implémentation Python d'upmpdcli.

## Fonctionnalités

- ✅ **Authentification** : Login avec username/password depuis la configuration
- ✅ **Catalogue** : Accès complet au catalogue Qobuz (albums, tracks, artistes, playlists)
- ✅ **Recherche** : Recherche dans le catalogue avec filtres
- ✅ **Favoris** : Accès aux albums, artistes, tracks et playlists favoris
- ✅ **Cache en mémoire** : Minimisation des requêtes API avec TTL configurable
- ✅ **Export DIDL** : Conversion automatique en format DIDL-Lite (UPnP/DLNA)
- ✅ **Integration pmocovers** : Cache automatique des images (feature `covers`)
- ✅ **Integration pmoaudiocache** : Cache audio haute résolution avec métadonnées (feature `cache`)
- ✅ **API HTTP** : Endpoints REST via pmoserver (feature `pmoserver`)

## Installation

Ajoutez la dépendance dans votre `Cargo.toml` :

```toml
[dependencies]
pmoqobuz = { path = "../pmoqobuz" }
```

## Configuration

Les credentials Qobuz doivent être configurés dans `.pmomusic.yml` :

```yaml
accounts:
  qobuz:
    username: "votre@email.com"
    password: "votre_mot_de_passe"
```

## Utilisation

### Exemple basique

```rust
use pmoqobuz::QobuzClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Connexion depuis la configuration
    let client = QobuzClient::from_config().await?;

    // Rechercher des albums
    let albums = client.search_albums("Miles Davis").await?;

    for album in albums.iter().take(5) {
        println!("{} - {}", album.artist.name, album.title);
    }

    Ok(())
}
```

### Export DIDL

```rust
use pmoqobuz::{QobuzClient, ToDIDL};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = QobuzClient::from_config().await?;

    let album = client.get_album("album_id").await?;
    let didl_container = album.to_didl_container("parent_id")?;

    let tracks = client.get_album_tracks(&album.id).await?;
    for track in tracks {
        let didl_item = track.to_didl_item(&didl_container.id)?;
        println!("{}", didl_item.title);
    }

    Ok(())
}
```

### Favoris

```rust
use pmoqobuz::QobuzClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = QobuzClient::from_config().await?;

    // Albums favoris
    let albums = client.get_favorite_albums().await?;
    println!("{} albums favoris", albums.len());

    // Artistes favoris
    let artists = client.get_favorite_artists().await?;

    // Tracks favorites
    let tracks = client.get_favorite_tracks().await?;

    // Playlists de l'utilisateur
    let playlists = client.get_user_playlists().await?;

    Ok(())
}
```

## Formats audio

Qobuz propose plusieurs formats :

| Format | Description | Format ID |
|--------|-------------|-----------|
| `Mp3_320` | MP3 320 kbps | 5 |
| `Flac_Lossless` | FLAC 16 bit / 44.1 kHz | 6 (défaut) |
| `Flac_HiRes_96` | FLAC 24 bit / jusqu'à 96 kHz | 7 |
| `Flac_HiRes_192` | FLAC 24 bit / jusqu'à 192 kHz | 27 |

```rust
use pmoqobuz::{QobuzClient, AudioFormat};

let mut client = QobuzClient::from_config().await?;
client.set_format(AudioFormat::Flac_HiRes_96);
```

## Cache

Le cache en mémoire utilise `moka` avec TTL :

- **Albums** : 1 heure
- **Tracks** : 1 heure
- **Artistes** : 1 heure
- **Playlists** : 30 minutes
- **Recherches** : 15 minutes
- **URLs de streaming** : 5 minutes

```rust
// Statistiques du cache
let stats = client.cache().stats().await;
println!("Albums: {}", stats.albums_count);
println!("Total: {}", stats.total_count());

// Vider le cache
client.cache().clear_all().await;
```

## Cache avancé (feature `cache`)

La feature `cache` active le support complet de pmocovers et pmoaudiocache pour télécharger et cacher localement les images et l'audio haute résolution :

```rust
use pmoqobuz::{QobuzSource, QobuzClient};
use pmocovers::Cache as CoverCache;
use pmoaudiocache::AudioCache;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize caches
    let cover_cache = Arc::new(CoverCache::new("./cache/covers", 500)?);
    let audio_cache = Arc::new(AudioCache::new("./cache/audio", 100)?);

    // Create source with caching
    let client = QobuzClient::from_config().await?;
    let source = QobuzSource::new_with_cache(
        client,
        "http://localhost:8080",
        Some(cover_cache),
        Some(audio_cache),
    );

    // Add tracks with automatic caching
    let tracks = source.client().get_favorite_tracks().await?;
    for track in tracks.iter().take(5) {
        let track_id = source.add_track(track).await?;
        // Audio and cover are now cached locally
        let uri = source.resolve_uri(&track_id).await?;
        println!("Cached: {}", uri);
    }

    Ok(())
}
```

**Métadonnées enrichies préservées** :
- Titre, artiste, album
- Numéro de piste et de disque
- Année de sortie
- Genre(s) et label
- Qualité audio (sample rate, bit depth, channels)
- Durée

## Exemples

Exécutez les exemples :

```bash
# Exemple basique
cargo run --example basic_usage

# Exemple avec cache (nécessite la feature cache)
cargo run --example with_cache --features cache
```

## Architecture

```
pmoqobuz/
├── src/
│   ├── lib.rs           # Module principal
│   ├── client.rs        # Client haut-niveau
│   ├── models.rs        # Structures de données
│   ├── api/
│   │   ├── mod.rs       # API client bas-niveau
│   │   ├── auth.rs      # Authentification
│   │   ├── catalog.rs   # Accès catalogue
│   │   └── user.rs      # API utilisateur
│   ├── cache.rs         # Cache en mémoire
│   ├── didl.rs          # Export DIDL-Lite
│   └── error.rs         # Gestion des erreurs
└── examples/
    └── basic_usage.rs   # Exemple d'utilisation
```

## Tests

```bash
cargo test -p pmoqobuz
```

## Documentation

Générez la documentation :

```bash
cargo doc -p pmoqobuz --open
```

## Features

- `covers` : Active pmocovers pour le cache d'images
- `cache` : Active pmocovers + pmoaudiocache pour le cache complet (images + audio)
- `pmoserver` : Active les endpoints REST via pmoserver

## Dépendances principales

- `reqwest` : Client HTTP
- `tokio` : Runtime asynchrone
- `serde` / `serde_json` : Sérialisation JSON
- `moka` : Cache en mémoire avec TTL
- `pmodidl` : Export DIDL-Lite
- `pmoconfig` : Configuration
- `pmocovers` : Cache d'images (optionnel)
- `pmoaudiocache` : Cache audio (optionnel)

## Licence

Ce code fait partie du projet PMOMusic.

## Références

- [API Qobuz Documentation](https://github.com/Qobuz/api-documentation)
- [upmpdcli Qobuz Plugin](https://www.lesbonscomptes.com/upmpdcli/)
