# Architecture du système de cache PMOMusic

## Vue d'ensemble

Le système de cache de PMOMusic est organisé en trois crates modulaires :

```
pmocache (générique)
    ├── DB générique avec collections
    └── Cache générique avec téléchargement

pmocovers (spécialisé images)
    ├── Utilise pmocache comme base
    └── Ajoute conversion WebP + variantes

pmoaudiocache (spécialisé audio)
    ├── Utilise pmocache comme base
    ├── Conversion automatique en FLAC (standardisation)
    └── Ajoute extraction métadonnées + collections d'albums
```

## Principes de conception

### 1. Synchronisation et partage

Les caches sont conçus pour être utilisés via `Arc<Cache>` :

```rust
// ✅ Bon usage
let cache = Arc::new(Cache::new(config)?);
let cache_clone = Arc::clone(&cache); // Clone léger de l'Arc

// ❌ Mauvais usage (Cache n'implémente pas Clone volontairement)
let cache = Cache::new(config)?;
let cache_clone = cache.clone(); // ❌ Erreur de compilation
```

Pourquoi cette approche ?
- `Cache` contient déjà des `Arc` internes (`Arc<DB>`, `Arc<Mutex<()>>`)
- Pas besoin de double niveau d'Arc (`Arc<Cache>` suffit)
- Les méthodes prennent `&self` et gèrent la synchronisation en interne
- Évite les clonages accidentels

### 2. Collections

Le système de collections permet de regrouper des éléments logiquement :

**Pour les images (pmocovers)** :
- Les collections ne sont généralement pas utilisées
- Chaque image a une clé unique basée sur son URL

**Pour l'audio (pmoaudiocache)** :
- Collections = albums (format : `"artist:album"`)
- Exemple : `"pink_floyd:wish_you_were_here"`
- Génération automatique depuis les métadonnées ID3

### 3. Base de données

Schéma SQLite commun :

```sql
CREATE TABLE {table_name} (
    pk TEXT PRIMARY KEY,        -- Clé unique (SHA1 de l'URL)
    source_url TEXT,            -- URL source
    collection TEXT,            -- Collection (optionnel)
    hits INTEGER DEFAULT 0,     -- Nombre d'accès
    last_used TEXT              -- Dernière utilisation (RFC3339)
);
```

Chaque cache a sa propre table :
- `pmocovers` → table "covers"
- `pmoaudiocache` → table "audio_tracks"

### 4. Stockage des fichiers

Structure sur disque :

```
cache_dir/
├── cache.db                    # Base SQLite
├── {pk}.{extension}            # Fichiers cachés
```

Extensions par type :
- Images : `{pk}.orig.webp` (conversion automatique depuis n'importe quel format d'image)
- Audio : `{pk}.flac` (conversion automatique depuis n'importe quel format audio)

## Utilisation

### Cache d'images (pmocovers)

```rust
use pmocovers::Cache;
use std::sync::Arc;

let cache = Arc::new(Cache::new("./covers_cache", 1000)?);

// Ajouter une image
let pk = cache.add_from_url("http://example.com/cover.jpg").await?;

// Récupérer une image
let path = cache.get(&pk).await?;
```

### Cache audio (pmoaudiocache)

```rust
use pmoaudiocache::AudioCache;
use std::sync::Arc;

let cache = Arc::new(AudioCache::new("./audio_cache", 1000)?);

// Ajouter une piste (métadonnées extraites automatiquement)
let (pk, metadata) = cache.add_from_url("http://example.com/track.flac").await?;

// Lister les collections (albums)
let collections = cache.list_collections().await?;

// Récupérer toutes les pistes d'un album
let tracks = cache.get_collection("pink_floyd:wish_you_were_here").await?;
```

### Intégration avec pmoserver

```rust
use pmocovers::CoverCacheExt;
use pmoaudiocache::AudioCacheExt;
use pmoserver::ServerBuilder;

let mut server = ServerBuilder::new_configured().build();

// Initialiser les caches
let covers = server.init_cover_cache_configured().await?;
let audio = server.init_audio_cache_configured().await?;

server.start().await;
```

## Avantages de cette architecture

1. **Modularité** : Chaque cache est indépendant
2. **Réutilisabilité** : `pmocache` peut être utilisé pour d'autres types de caches
3. **Performance** : Utilisation d'`Arc` pour un partage efficace
4. **Sécurité** : Pas de `Clone` accidentel, synchronisation explicite
5. **Extensibilité** : Facile d'ajouter de nouveaux types de caches

## Exemple de nouveau cache

Pour créer un nouveau type de cache (par exemple pour des vidéos) :

```rust
use pmocache::{Cache as GenericCache, CacheConfig};
use std::sync::Arc;

pub struct VideoCache {
    cache: GenericCache,
    // Champs spécifiques aux vidéos
}

impl VideoCache {
    pub fn new(dir: &str, limit: usize) -> Result<Self> {
        let config = CacheConfig::new(dir, limit, "videos", "mp4");
        let cache = GenericCache::new(config)?;

        Ok(Self { cache })
    }

    // Méthodes spécifiques aux vidéos
    pub async fn add_with_transcoding(&self, url: &str) -> Result<String> {
        // Télécharger, transcoder, puis utiliser self.cache.add()
        todo!()
    }
}
```
