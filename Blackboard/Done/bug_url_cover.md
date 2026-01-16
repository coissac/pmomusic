# Bug URL relative de la cover - TERMINÉ

## Description du bug

Le document DIDL généré par les PMO playlists possédait une URL absolue pour le flux audio, mais une URL relative pour la cover. Les deux entités (flux audio et cover) sont stockées dans des caches `pmoaudiocache` et `pmocovers` respectivement.

## Crates concernées

- **pmoqobuz** (crate corrigée)
- pmoplaylist, pmocache, pmoaudiocache, pmocovers, pmodidl (explorées pour l'analyse)

## Analyse

### Architecture des URLs dans PMOMusic

1. **`pmocache::FileCache::route_for`** retourne une route **relative** (ex: `/audio/flac/abc123`, `/covers/images/def456`)

2. **Les sources (`pmoqobuz`, `pmoparadise`)** reçoivent un `base_url` à leur création et sont responsables de convertir les URLs relatives en absolues avant de retourner les résultats de Browse.

3. **`pmoplaylist::ReadHandle::to_items`** génère des items DIDL avec des URLs relatives pour l'audio ET la cover.

### Localisation du bug

Le bug se trouvait dans **`pmoqobuz/src/source.rs`** dans la fonction `adapt_playlist_items_to_qobuz`.

Cette fonction convertissait correctement l'URL audio en URL absolue mais **oubliait de faire la même conversion pour `item.album_art`** (la cover).

### Comparaison avec pmoparadise

`pmoparadise/src/source.rs` faisait déjà correctement les deux conversions (audio et cover).

## Correction appliquée

### Fichier modifié

- `pmoqobuz/src/source.rs`

### Modification

Ajout de la conversion de l'URL de la cover dans `adapt_playlist_items_to_qobuz` :

```rust
// Convertir l'URL de la cover en URL absolue si elle est relative
if let Some(art) = item.album_art.as_mut() {
    if art.starts_with('/') {
        *art = format!("{}{}", self.inner.base_url, art);
    }
}
```

## Pattern architectural

Chaque source qui utilise `pmoplaylist::to_items()` doit post-traiter les items pour convertir les URLs relatives (`/audio/...`, `/covers/...`) en URLs absolues en utilisant son `base_url`.

## Statut

**TERMINÉ** - La compilation réussit après la correction.
