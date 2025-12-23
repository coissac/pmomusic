# Utilisation du cache disque pour favoris/bibliothèque

## Intégration dans QobuzClient

### Étape 1 : Ajouter le cache disque au client

```rust
// Dans src/client.rs

use crate::disk_cache::DiskCache;

pub struct QobuzClient {
    api: QobuzApi,
    cache: Arc<QobuzCache>,          // Cache mémoire (existant)
    disk_cache: Arc<DiskCache>,      // Cache disque (nouveau)
    auth_info: Option<AuthInfo>,
}

impl QobuzClient {
    pub async fn from_config_obj(config: &Config) -> Result<Self> {
        // ... code existant ...

        // Créer le cache disque (utilise le répertoire configuré)
        let disk_cache_dir = config.get_qobuz_cache_dir()?;
        let disk_cache = Arc::new(DiskCache::new(disk_cache_dir)?);

        Ok(Self {
            api,
            cache: Arc::new(QobuzCache::new()),
            disk_cache,
            auth_info: Some(auth_info),
        })
    }
}
```

### Étape 2 : Utiliser le cache pour get_favorite_albums

```rust
// Dans src/client.rs

impl QobuzClient {
    /// Récupère les albums favoris (avec cache disque)
    pub async fn get_favorite_albums(&self) -> Result<Vec<Album>> {
        let user_id = self.auth_info
            .as_ref()
            .map(|a| &a.user_id)
            .ok_or_else(|| QobuzError::Unauthorized("Not authenticated".to_string()))?;

        let cache_key = format!("favorites_albums_{}", user_id);

        // 1. Essayer de charger depuis le cache disque (TTL: 1 heure)
        if let Ok(Some(albums)) = self.disk_cache.load_with_ttl::<Vec<Album>>(
            &cache_key,
            Duration::from_secs(3600)
        ) {
            info!("✓ Loaded {} favorite albums from disk cache", albums.len());
            return Ok(albums);
        }

        // 2. Sinon, requête API
        info!("Fetching favorite albums from API...");
        let albums = self.api.get_favorite_albums().await?;

        // 3. Sauvegarder dans le cache disque
        if let Err(e) = self.disk_cache.save(&cache_key, &albums) {
            debug!("Failed to save favorites to disk cache: {}", e);
        } else {
            info!("✓ Saved {} favorite albums to disk cache", albums.len());
        }

        Ok(albums)
    }

    /// Récupère les tracks favoris (avec cache disque)
    pub async fn get_favorite_tracks(&self) -> Result<Vec<Track>> {
        let user_id = self.auth_info
            .as_ref()
            .map(|a| &a.user_id)
            .ok_or_else(|| QobuzError::Unauthorized("Not authenticated".to_string()))?;

        let cache_key = format!("favorites_tracks_{}", user_id);

        // 1. Cache disque (TTL: 1 heure)
        if let Ok(Some(tracks)) = self.disk_cache.load_with_ttl::<Vec<Track>>(
            &cache_key,
            Duration::from_secs(3600)
        ) {
            info!("✓ Loaded {} favorite tracks from disk cache", tracks.len());
            return Ok(tracks);
        }

        // 2. API
        info!("Fetching favorite tracks from API...");
        let tracks = self.api.get_favorite_tracks().await?;

        // 3. Sauvegarder
        if let Err(e) = self.disk_cache.save(&cache_key, &tracks) {
            debug!("Failed to save favorites to disk cache: {}", e);
        } else {
            info!("✓ Saved {} favorite tracks to disk cache", tracks.len());
        }

        Ok(tracks)
    }

    /// Récupère les playlists (avec cache disque)
    pub async fn get_user_playlists(&self) -> Result<Vec<Playlist>> {
        let user_id = self.auth_info
            .as_ref()
            .map(|a| &a.user_id)
            .ok_or_else(|| QobuzError::Unauthorized("Not authenticated".to_string()))?;

        let cache_key = format!("playlists_{}", user_id);

        // 1. Cache disque (TTL: 30 minutes - les playlists changent plus souvent)
        if let Ok(Some(playlists)) = self.disk_cache.load_with_ttl::<Vec<Playlist>>(
            &cache_key,
            Duration::from_secs(1800)
        ) {
            info!("✓ Loaded {} playlists from disk cache", playlists.len());
            return Ok(playlists);
        }

        // 2. API
        info!("Fetching playlists from API...");
        let playlists = self.api.get_user_playlists().await?;

        // 3. Sauvegarder
        if let Err(e) = self.disk_cache.save(&cache_key, &playlists) {
            debug!("Failed to save playlists to disk cache: {}", e);
        } else {
            info!("✓ Saved {} playlists to disk cache", playlists.len());
        }

        Ok(playlists)
    }

    /// Invalide le cache des favoris (après ajout/suppression)
    pub async fn invalidate_favorites_cache(&self) -> Result<()> {
        let user_id = self.auth_info
            .as_ref()
            .map(|a| &a.user_id)
            .ok_or_else(|| QobuzError::Unauthorized("Not authenticated".to_string()))?;

        self.disk_cache.invalidate(&format!("favorites_albums_{}", user_id))?;
        self.disk_cache.invalidate(&format!("favorites_tracks_{}", user_id))?;
        self.disk_cache.invalidate(&format!("playlists_{}", user_id))?;

        info!("✓ Invalidated favorites cache");
        Ok(())
    }
}
```

### Étape 3 : Méthodes utilitaires

```rust
impl QobuzClient {
    /// Retourne des statistiques sur le cache disque
    pub fn disk_cache_stats(&self) -> Result<(usize, u64)> {
        let count = self.disk_cache.count()?;
        let size = self.disk_cache.size()?;
        Ok((count, size))
    }

    /// Vide complètement le cache disque
    pub fn clear_disk_cache(&self) -> Result<()> {
        self.disk_cache.clear_all()
    }
}
```

## Structure sur disque

```
.pmomusic/
├── config.yaml
└── cache/
    └── qobuz/
        ├── favorites_albums_1217710.json    # 375 albums (~200 KB)
        ├── favorites_tracks_1217710.json    # Tracks favoris
        └── playlists_1217710.json           # Playlists utilisateur
```

## Bénéfices

### Sans cache disque (AVANT)
```bash
# Lancement 1
INFO  Fetching 375 favorite albums from API... (2.5s)

# Lancement 2 (app redémarrée)
INFO  Fetching 375 favorite albums from API... (2.5s) ← Requête inutile !

# Lancement 3
INFO  Fetching 375 favorite albums from API... (2.5s) ← Requête inutile !
```

**Total** : 3 requêtes API × 2.5s = **7.5 secondes**

### Avec cache disque (APRÈS)
```bash
# Lancement 1 (cache miss)
INFO  Fetching 375 favorite albums from API... (2.5s)
INFO  ✓ Saved 375 favorite albums to disk cache

# Lancement 2 (cache hit!)
INFO  ✓ Loaded 375 favorite albums from disk cache (5ms) ← Instantané !

# Lancement 3 (cache hit!)
INFO  ✓ Loaded 375 favorite albums from disk cache (5ms) ← Instantané !
```

**Total** : 1 requête API × 2.5s + 2 cache hits × 5ms = **2.51 secondes**

**Amélioration** : **66% plus rapide** + réduction de **66% des requêtes API**

## TTL recommandés

| Donnée | TTL | Justification |
|--------|-----|---------------|
| Albums favoris | 1h | Changent rarement |
| Tracks favoris | 1h | Changent rarement |
| Playlists | 30min | Modifiées plus souvent |
| Bibliothèque complète | 24h | Très volumineuse, change peu |

## Invalidation intelligente

Invalider le cache après modifications :

```rust
// Après ajout d'un favori
client.add_favorite_album("123").await?;
client.invalidate_favorites_cache().await?;

// Après suppression
client.remove_favorite_album("123").await?;
client.invalidate_favorites_cache().await?;
```

## Tests

```bash
# Test du cache disque
cargo test -p pmoqobuz disk_cache

# Test d'intégration
cargo run --example basic_usage

# Logs détaillés
RUST_LOG=info,pmoqobuz::disk_cache=debug cargo run --example basic_usage
```

## Migration

Pour ajouter le cache disque au client existant :

1. Ajouter le champ `disk_cache` à `QobuzClient`
2. Initialiser dans `from_config_obj()`
3. Modifier `get_favorite_albums()`, `get_favorite_tracks()`, etc.
4. Tester avec des gros catalogues (375+ albums)

## Taille estimée du cache

Pour un utilisateur avec :
- 375 albums favoris
- 100 tracks favoris
- 10 playlists

**Taille totale** : ~300 KB (négligeable)

## Comparaison : pmocache vs DiskCache

| Critère | pmocache | DiskCache |
|---------|----------|-----------|
| **Complexité** | Élevée (SQLite, download, variants) | Faible (fichiers JSON simples) |
| **Taille overhead** | ~100 KB (SQLite + tables) | 0 (juste les JSON) |
| **Performance** | Excellent pour binaires | Excellent pour JSON |
| **Maintenance** | Complexe | Simple |
| **Adapté pour JSON** | ❌ Non | ✅ Oui |

**Conclusion** : `DiskCache` est **parfaitement adapté** pour le cache de favoris/bibliothèque.
