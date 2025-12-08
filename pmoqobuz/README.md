# pmoqobuz - Client Rust pour l'API Qobuz

Client Rust pour l'API Qobuz avec intégration automatique du Spoofer pour obtenir des AppID et secrets valides.

## 🎯 Fonctionnalités

- ✅ **Authentification** automatique avec credentials
- ✅ **Spoofer intégré** - Obtention automatique d'AppID et secrets valides
- ✅ **Signatures MD5** pour les requêtes sensibles (streaming, bibliothèque)
- ✅ **Cache** en mémoire pour optimiser les performances
- ✅ **Support DIDL-Lite** pour l'export UPnP/DLNA
- ✅ **Recherche** dans le catalogue (albums, artistes, tracks, playlists)
- ✅ **Favoris** et playlists utilisateur
- ✅ **Désérialisation robuste** (gère integers et strings pour les IDs)

## 🚀 Utilisation rapide

### Configuration minimale

```yaml
# ~/.pmomusic/config.yaml
accounts:
  qobuz:
    username: "your_email@example.com"
    password: "your_password"
    # AppID et secret seront automatiquement obtenus via le Spoofer
```

### Code d'exemple

```rust
use pmoqobuz::QobuzClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Le Spoofer s'exécute automatiquement si nécessaire
    let client = QobuzClient::from_config().await?;

    // Rechercher des albums
    let albums = client.search_albums("Miles Davis").await?;
    for album in albums.iter().take(5) {
        println!("{} - {}", album.artist.name, album.title);
    }

    Ok(())
}
```

## 📖 Documentation

- [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) - Statut d'implémentation complet
- [API_ANALYSIS.md](API_ANALYSIS.md) - Analyse des différences avec l'API Python
- [examples/basic_usage.rs](examples/basic_usage.rs) - Exemple complet
- [examples/spoofer.rs](examples/spoofer.rs) - Utilisation manuelle du Spoofer
