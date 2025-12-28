# Radio Paradise - Architecture

## Vue d'ensemble

Cette crate fournit deux architectures pour accéder à Radio Paradise :

1. **RadioParadiseStreamSource** (legacy) : Télécharge les blocs FLAC entiers et découpe manuellement
2. **RadioParadisePlaylistFeeder** (recommandé) : Utilise les URLs gapless individuelles + système de playlist

## RadioParadisePlaylistFeeder (Architecture simplifiée)

### Principe

Au lieu de télécharger un gros bloc FLAC contenant plusieurs chansons et de calculer manuellement les bornes de chaque chanson, cette architecture :

1. Récupère le bloc via l'API `get_block`
2. Filtre les chansons : garde uniquement celles où `sched_time_millis + duration >= now()`
3. Télécharge chaque chanson individuellement via son `gapless_url`
4. Stocke les métadonnées (titre, artiste, album, cover) dans le cache audio
5. Push les PKs dans une playlist avec TTL calculé = `sched_end - now()`
6. La playlist est consommée par `PlaylistSource` qui produit le flux audio

### Avantages

- **Simplicité** : Pas de calcul de bornes, pas de découpe manuelle
- **Précision** : Chaque fichier FLAC = une chanson exactement
- **Réutilisabilité** : Utilise l'infrastructure existante (pmoplaylist, pmoaudiocache, PlaylistSource)
- **TTL automatique** : Les chansons expirées sont automatiquement retirées de la playlist

### Exemple d'utilisation

```rust
use pmoparadise::{RadioParadiseClient, RadioParadisePlaylistFeeder};
use pmoaudiocache::cache::new_cache;
use pmocovers::cache::new_cache as new_covers_cache;
use pmoaudio_ext::PlaylistSource;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Créer les caches
    let audio_cache = Arc::new(new_cache("./cache/audio", 500)?);
    let covers_cache = Arc::new(new_covers_cache("./cache/covers", 500)?);

    // Créer le client Radio Paradise
    let client = RadioParadiseClient::new().await?;

    // Créer le feeder (retourne feeder + read_handle)
    let (feeder, read_handle) = RadioParadisePlaylistFeeder::new(
        client.clone(),
        audio_cache.clone(),
        covers_cache.clone(),
        "rp-live".to_string(),
        Some("radio-paradise".to_string()),
    ).await?;

    // Lancer le feeder dans une tâche
    let feeder = Arc::new(feeder);
    let feeder_clone = feeder.clone();
    tokio::spawn(async move {
        if let Err(e) = feeder_clone.run().await {
            tracing::error!("Feeder error: {}", e);
        }
    });

    // Enqueue le bloc actuel
    let now_playing = client.now_playing().await?;
    feeder.push_block_id(now_playing.block.event);

    // Créer la source audio depuis la playlist
    let playlist_source = PlaylistSource::new(read_handle, audio_cache);

    // Utiliser playlist_source dans un pipeline pmoaudio...

    Ok(())
}
```

## Radio Paradise API - Référence des URLs

### URLs d'artistes

**Format** : `https://radioparadise.com/music/artist/{artist_id}`
**Format alternatif** : `https://radioparadise.com/music/artist/{artist_id}/{Artist_Name}`

Le champ `artist_id` est disponible dans `song.song_credit_list[].artist_id`.

**Exemples** :
- Sting (ID 4247) : https://radioparadise.com/music/artist/4247
- Pink Martini (ID 3718) : https://radioparadise.com/music/artist/3718/Pink_Martini

### URLs de chansons

**Format** : `https://legacy.radioparadise.com/rp3.php?file=songinfo&name=Music&song_id={song_id}`

Le champ `song_id` est disponible dans `song.song_id`.

### URLs gapless (FLAC individuels)

**Format** : Fourni directement par l'API dans `song.gapless_url`

**Exemple** : `https://audio-geo.radioparadise.com/chan/1/x/1065/4/g/1065-3.flac`

Ces URLs pointent vers des fichiers FLAC contenant **une seule chanson**, permettant un téléchargement et un traitement simplifiés.

### Timestamps (`sched_time_millis`)

Tous les timestamps de l'API Radio Paradise sont en **UTC** (Unix timestamp en millisecondes).

**Exemple** :
```json
"sched_time_millis": 1763272707000  // 2025-11-16 06:16:09 UTC
```

Pour calculer la fin de diffusion d'une chanson :
```rust
let sched_end = song.sched_time_millis + song.duration;
let is_still_playing = sched_end >= now_ms;
```

## Notes d'implémentation future

Ces URLs peuvent être utilisées pour :
- **Enrichir les métadonnées** avec les biographies d'artistes (scraping des pages artistes)
- **Récupérer les paroles** (via l'API ou scraping)
- **Afficher l'historique de diffusion** par chanson
- **Lier vers les pages communautaires** Radio Paradise pour ratings/commentaires
- **Intégration MusicBrainz/Discogs** : utiliser `asin` ou rechercher par artiste+titre+album

## Structure des données

### Block

Un bloc Radio Paradise contient :
- `event` : ID de début du bloc
- `end_event` : ID de fin (= event du bloc suivant)
- `length` : Durée totale en millisecondes
- `url` : URL du bloc FLAC complet (legacy)
- `song` : Map des chansons indexées par position ("0", "1", "2", ...)

### Song

Chaque chanson contient :
- **Métadonnées** : `title`, `artist`, `album`, `year`, `rating`
- **Timing** : `elapsed` (position dans le bloc), `duration`, `sched_time_millis`
- **Identifiants** : `song_id`, `audio_id`, `event`
- **Covers** : `cover`, `cover_large`, `cover_medium`, `cover_small`
- **Streaming** : `gapless_url` (⭐ nouveau, recommandé)
- **Artiste** : `artist_id` (pour construire les URLs)

### Filtrage des chansons

Pour éviter de télécharger des chansons déjà terminées :

```rust
let now_ms = Instant::now()
    .duration_since(UNIX_EPOCH)?
    .as_millis() as u64;

for (idx, song) in block.songs_ordered() {
    if song.is_still_playing(now_ms) {
        // Télécharger et ajouter à la playlist
    }
}
```
