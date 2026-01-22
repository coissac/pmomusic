# Rapport : Implémentation de pmoradiofrance (Round 3)

**Date** : 2026-01-22  
**Crate** : `pmoradiofrance`  
**Statut** : Implémentation initiale complète

---

## Résumé

Création de la crate `pmoradiofrance` qui fournit un client Rust pour accéder aux APIs publiques de Radio France. Le client permet :

- La découverte dynamique de toutes les stations (~70+)
- La récupération des métadonnées live (émission en cours, producteur, visuels)
- L'accès aux flux audio HiFi (AAC 192 kbps, HLS)
- Le cache de la liste des stations avec TTL configurable

---

## Fichiers créés

| Fichier | Description |
|---------|-------------|
| `pmoradiofrance/Cargo.toml` | Configuration de la crate avec dépendances |
| `pmoradiofrance/src/lib.rs` | Point d'entrée et exports publics |
| `pmoradiofrance/src/error.rs` | Types d'erreurs (`Error`, `Result`) |
| `pmoradiofrance/src/models.rs` | Structures de données pour l'API |
| `pmoradiofrance/src/client.rs` | `RadioFranceClient` et `ClientBuilder` |
| `pmoradiofrance/src/config_ext.rs` | Extension `RadioFranceConfigExt` pour pmoconfig |
| `pmoradiofrance/examples/discover_stations.rs` | Exemple de découverte |
| `pmoradiofrance/examples/live_metadata.rs` | Exemple de métadonnées live |

## Fichiers modifiés

| Fichier | Modification |
|---------|--------------|
| `Cargo.toml` (racine) | Ajout de `pmoradiofrance` au workspace |

---

## Architecture du client

### `RadioFranceClient`

Client HTTP stateless pour interroger les APIs Radio France :

```rust
// Création
let client = RadioFranceClient::new().await?;

// Découverte des stations
let stations = client.discover_all_stations().await?;

// Métadonnées live
let metadata = client.live_metadata("franceculture").await?;

// URL du flux HiFi
let stream_url = client.get_hifi_stream_url("fip_rock").await?;
```

### Découverte des stations

Le client découvre dynamiquement :

1. **Stations principales** (7) : France Inter, France Info, France Culture, France Musique, FIP, Mouv', France Bleu
2. **Webradios** (~15-20) : FIP Rock, FIP Jazz, France Musique Baroque, etc.
3. **Radios locales France Bleu** (~44) : via le champ `now.localRadios` de l'API

### Gestion des webradios

Le parsing des slugs gère automatiquement les webradios :

| Slug | Base station | Paramètre webradio |
|------|--------------|-------------------|
| `fip` | `fip` | - |
| `fip_rock` | `fip` | `?webradio=fip_rock` |
| `francemusique_jazz` | `francemusique` | `?webradio=francemusique_jazz` |
| `francebleu_alsace` | `francebleu_alsace` | - (slug direct) |

---

## Extension de configuration

Le trait `RadioFranceConfigExt` permet de cacher la liste des stations :

```rust
use pmoconfig::get_config;
use pmoradiofrance::RadioFranceConfigExt;

let config = get_config();

// Vérifier le cache (TTL par défaut : 7 jours)
if let Some(stations) = config.get_radiofrance_stations_cached()? {
    // Utiliser les stations du cache
} else {
    // Découvrir et mettre en cache
    let client = RadioFranceClient::new().await?;
    let stations = client.discover_all_stations().await?;
    config.set_radiofrance_cached_stations(&stations)?;
}
```

### Configuration YAML générée

```yaml
sources:
  radiofrance:
    enabled: true
    station_cache_ttl_secs: 604800  # 7 jours
    station_cache:
      stations: [...]
      last_updated: 1769112000
      version: 1
```

---

## Tests d'intégration

17 tests d'intégration qui appellent la vraie API Radio France :

```bash
# Exécuter tous les tests d'intégration
cargo test -p pmoradiofrance -- --ignored

# Avec output visible
cargo test -p pmoradiofrance -- --ignored --nocapture
```

| Test | Description | Status |
|------|-------------|--------|
| `test_client_creation` | Création du client | ✅ |
| `test_live_metadata_franceculture` | Métadonnées France Culture | ✅ |
| `test_live_metadata_franceinter` | Métadonnées France Inter | ✅ |
| `test_live_metadata_fip` | Métadonnées FIP (avec chanson) | ✅ |
| `test_live_metadata_fip_rock` | Métadonnées webradio FIP Rock | ✅ |
| `test_live_metadata_francemusique` | Métadonnées France Musique | ✅ |
| `test_live_metadata_francebleu` | Métadonnées + radios locales | ✅ |
| `test_live_metadata_mouv` | Métadonnées Mouv' | ✅ |
| `test_get_hifi_stream_url` | URLs des flux HiFi | ✅ |
| `test_get_available_streams` | Liste des flux disponibles | ✅ |
| `test_discover_main_stations` | Découverte stations principales | ✅ |
| `test_discover_fip_webradios` | Découverte webradios FIP | ✅ |
| `test_discover_francemusique_webradios` | Découverte webradios FM | ✅ |
| `test_discover_local_radios` | Découverte radios locales | ✅ |
| `test_discover_all_stations` | Découverte complète | ✅ |
| `test_invalid_station` | Gestion d'erreur | ✅ |
| `test_refresh_delay` | Calcul délai refresh | ✅ |

---

## Correction effectuée pendant l'implémentation

**Bug découvert** : Le champ `localRadios` de l'API France Bleu est dans `now.localRadios` (imbriqué dans `ShowMetadata`), pas au niveau racine de `LiveResponse`.

**Correction** :
1. Déplacé le champ `local_radios` de `LiveResponse` vers `ShowMetadata`
2. Ajouté une méthode helper `LiveResponse::local_radios()` pour accès simplifié
3. Mis à jour le client et les tests

---

## Features Cargo

| Feature | Description | Dépendances |
|---------|-------------|-------------|
| `default` | Configuration de base | `pmoconfig` |
| `pmoconfig` | Support extension config | `dep:pmoconfig` |
| `cache` | Support cache audio/covers | `pmocovers`, `pmoaudiocache` |
| `playlist` | Support playlists FIFO | `pmoplaylist` |
| `logging` | Logs avec tracing | - |
| `server` | Support serveur complet | Toutes les features |
| `full` | Toutes les features | `server`, `logging` |

---

## Prochaines étapes

1. **Implémenter `source.rs`** : Trait `MusicSource` pour intégration UPnP
2. **Ajouter support FIFO** : Pour les radios musicales (FIP, France Musique)
3. **Cache des métadonnées live** : Respecter `delayToRefresh`
4. **Intégration serveur** : Routes API REST via pmoserver

---

## Exemples d'utilisation

### Découverte des stations

```bash
cargo run -p pmoradiofrance --example discover_stations
```

### Métadonnées live

```bash
# Station par défaut (France Culture)
cargo run -p pmoradiofrance --example live_metadata

# Station spécifique
cargo run -p pmoradiofrance --example live_metadata -- fip_rock
```

---

## Round 4bis : Client Stateful et Support Playlist (2026-01-22)

### Objectif

Compléter l'implémentation avec un client stateful qui gère automatiquement le cache et les structures pour la génération de playlists UPnP.

### Fichiers créés

| Fichier | Description |
|---------|-------------|
| `pmoradiofrance/src/stateful_client.rs` | `RadioFranceStatefulClient` avec cache automatique |
| `pmoradiofrance/src/playlist.rs` | Structures pour playlists UPnP (groupes, items) |

### Fichiers modifiés

| Fichier | Modification |
|---------|--------------|
| `pmoradiofrance/src/lib.rs` | Ajout modules `stateful_client` et `playlist` + re-exports |
| `pmoradiofrance/src/error.rs` | Ajout variant `Config(#[from] anyhow::Error)` pour conversion |
| `pmoradiofrance/Cargo.toml` | Ajout dépendance `pmodidl` pour feature `playlist` |

---

### RadioFranceStatefulClient

Client de haut niveau avec gestion automatique du cache :

```rust
use pmoradiofrance::RadioFranceStatefulClient;
use pmoconfig::get_config;

let config = get_config();
let client = RadioFranceStatefulClient::new(config).await?;

// Cache automatique des stations (7 jours par défaut)
let stations = client.get_stations().await?;

// Cache intelligent des métadonnées (respecte delayToRefresh)
let metadata = client.get_live_metadata("franceculture").await?;
```

**Caractéristiques** :

- **Cache à deux niveaux** :
  - Liste des stations : persisté dans pmoconfig (7 jours)
  - Métadonnées live : en mémoire (TTL dynamique de l'API)
  
- **Thread-safe** : Clone + Send + Sync via `Arc<RwLock<...>>`

- **Gestion intelligente du TTL** : 
  - Stations : configurable via `set_station_cache_ttl()`
  - Métadonnées : utilise `delayToRefresh` de l'API

### Structures de Playlist

#### StationGroups

Organisation hiérarchique des stations pour navigation UPnP :

```rust
pub struct StationGroups {
    pub standalone: Vec<Station>,        // Sans webradios
    pub with_webradios: Vec<StationGroup>, // Avec webradios
    pub local_radios: Vec<Station>,      // France Bleu/ICI
}

pub struct StationGroup {
    pub main: Station,
    pub webradios: Vec<Station>,
}
```

**Logique de groupement** :
- Stations standalone : France Inter, France Culture, France Info, Mouv'
- Groupes avec webradios : FIP (+ FIP Rock, Jazz...), France Musique (+ variantes)
- Radios locales : ~44 radios ICI (ex-France Bleu)

#### StationPlaylist

Playlist UPnP volatile pour une station :

```rust
pub struct StationPlaylist {
    pub id: String,
    pub station: Station,
    pub stream_item: Item,  // Item UPnP avec métadonnées
}
```

**Mapping des métadonnées vers UPnP** :

| Type | title | artist | album | class |
|------|-------|--------|-------|-------|
| **Radio parlée** | émission • titre | producteur | émission | audioBroadcast |
| **Radio musicale** | titre chanson | artiste(s) | album | musicTrack |

**Gestion des covers** :
- Extraction UUID depuis `visual_background`
- Cache via `pmocovers` (optionnel)
- URLs Pikapi haute résolution (Large: 560x960)

---

### Corrections et améliorations

#### 1. Gestion des erreurs

**Problème** : Les méthodes `pmoconfig` retournent `anyhow::Result` mais le client utilise son propre type `Result<T, Error>`.

**Solution** : Ajout d'un variant dans `Error` pour conversion automatique :
```rust
pub enum Error {
    // ...
    #[error("Configuration error: {0}")]
    Config(#[from] anyhow::Error),
}
```

#### 2. Feature playlist

**Ajout** : Dépendance `pmodidl` pour les structures DIDL-Lite (Item, Resource) :
```toml
[features]
playlist = ["dep:pmoplaylist", "dep:pmodidl"]
```

#### 3. Doctests propres

**Problème initial** : Exemples marqués `ignore` mais testés avec `--include-ignored`.

**Solution** : Utilisation de `no_run` avec contexte async complet :
```rust
/// ```no_run
/// use pmoradiofrance::RadioFranceStatefulClient;
/// use pmoconfig::get_config;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = get_config();
///     let client = RadioFranceStatefulClient::new(config).await?;
///     // ...
///     Ok(())
/// }
/// ```
```

**Avantages** :
- Exemples compilés (vérification syntaxe)
- Pas exécutés (pas de dépendance réseau)
- Lignes de contexte cachées avec `#` dans la doc générée

#### 4. Conditional compilation propre

**Feature `logging`** pour le debug :
```rust
#[cfg(feature = "logging")]
fn remaining_ttl(&self) -> Duration { ... }

#[cfg(feature = "logging")]
tracing::debug!("Using cached metadata for {} (TTL: {:?})", 
    station, entry.remaining_ttl());
```

---

### Tests

**Résultats** :
- ✅ Tests unitaires : **26/26 passés**
- ✅ Tests d'intégration (API réelle) : **26/26 passés**
- ✅ Doctests : **12/12 compilés**

```bash
# Tests complets (unitaires + intégration + doctests)
cargo test -p pmoradiofrance -- --include-ignored

# Tests unitaires uniquement
cargo test -p pmoradiofrance --lib
```

---

### Règles métier importantes

1. **URLs de stream constantes** : L'URL du stream ne change JAMAIS, seules les métadonnées changent
2. **Polling intelligent** : Toujours respecter `delayToRefresh` de l'API
3. **Renommage France Bleu → ICI** : Les slugs restent `francebleu_*` mais l'affichage utilise "ICI"
4. **Validation du cache** : Triple vérification (existence + TTL + version d'algorithme)

---

### Prochaines étapes

1. **Implémenter `source.rs`** : Trait `MusicSource` pour intégration UPnP
   - Génération automatique des playlists via `StationGroups`
   - Rafraîchissement périodique des métadonnées (respecte `delayToRefresh`)
   - Gestion des streams live continus (pas de FIFO - API ne fournit que des flux)
   
2. **Intégration serveur** : Routes API REST via `pmoserver`
   - `/radiofrance/stations` : Liste des stations groupées
   - `/radiofrance/{slug}/metadata` : Métadonnées live avec cache
   - `/radiofrance/{slug}/stream` : Redirection vers flux HiFi
   - Cache registry pour les covers

3. **Optimisations** :
   - Pool de connexions HTTP partagé entre instances
   - Préchargement intelligent des métadonnées (stations populaires)
   - Métriques de cache (hit rate, age, refresh count)
   - Compression des réponses API

---

**Fin du rapport Round 4bis**
