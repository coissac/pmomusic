# API Radio France - Documentation complète

## Vue d'ensemble

Radio France expose plusieurs APIs publiques **sans authentification** pour accéder aux métadonnées des émissions en direct et aux flux audio.

**Date d'analyse :** 2026-01-22  
**Sources :** Analyse de fichiers HAR + tests directs

---

## 1. API Live par station

### Format général
```
https://www.radiofrance.fr/{station}/api/live?
```

### Stations disponibles

| Station | Endpoint | Status |
|---------|----------|--------|
| France Inter | `/franceinter/api/live?` | ✅ Fonctionne |
| France Info | `/franceinfo/api/live?` | ✅ Fonctionne |
| France Culture | `/franceculture/api/live?` | ✅ Fonctionne |
| France Musique | `/francemusique/api/live?` | ✅ Fonctionne |
| FIP | `/fip/api/live?` | ✅ Fonctionne |
| Mouv' | `/mouv/api/live?` | ✅ Fonctionne |
| France Bleu (national) | `/francebleu/api/live?` | ✅ Fonctionne |
| Mon Petit France Inter | `/monpetitfranceinter/api/live?` | ✅ Fonctionne |

### Structure de réponse

```json
{
  "stationName": "franceculture",
  "delayToRefresh": 262000,
  "migrated": true,
  "now": {
    "printProgMusic": true,
    "startTime": 1769108400,
    "endTime": 1769110122,
    "producer": "Nom du producteur",
    "firstLine": {
      "title": "Nom de l'émission",
      "id": "uuid-emission",
      "path": "franceculture/podcasts/emission"
    },
    "secondLine": {
      "title": "Titre de l'épisode/chronique",
      "id": "uuid-episode",
      "path": "franceculture/podcasts/emission/episode"
    },
    "thirdLine": {
      "title": "Sous-titre éventuel",
      "id": "uuid",
      "path": null
    },
    "intro": "Description de l'émission...",
    "reactAvailable": false,
    "visualBackground": {
      "model": "EmbedImage",
      "src": "https://www.radiofrance.fr/pikapi/images/uuid",
      "width": 4000,
      "height": 1000,
      "dominant": "#c8e8f8",
      "copyright": "Radio France"
    },
    "song": {
      "id": "uuid-morceau",
      "year": 2024,
      "interpreters": ["Artiste"],
      "release": {
        "label": "Label",
        "title": "Album",
        "reference": null
      }
    },
    "media": {
      "sources": [
        {
          "url": "https://icecast.radiofrance.fr/franceculture-lofi.mp3?id=radiofrance",
          "broadcastType": "live",
          "format": "mp3",
          "bitrate": 32
        },
        {
          "url": "https://stream.radiofrance.fr/franceculture/franceculture.m3u8?id=radiofrance",
          "broadcastType": "live",
          "format": "hls",
          "bitrate": 0
        },
        {
          "url": "https://icecast.radiofrance.fr/franceculture-hifi.aac?id=radiofrance",
          "broadcastType": "live",
          "format": "aac",
          "bitrate": 192
        },
        {
          "url": "https://icecast.radiofrance.fr/franceculture-midfi.aac?id=radiofrance",
          "broadcastType": "live",
          "format": "aac",
          "bitrate": 128
        },
        {
          "url": "https://stream.radiofrance.fr/franceculture/franceculture.m3u8?id=radiofrance",
          "broadcastType": "timeshift",
          "format": "hls",
          "bitrate": 0
        }
      ]
    },
    "localRadios": [],
    "visuals": {
      "card": { /* Image pour la carte */ },
      "player": { /* Image pour le player */ }
    }
  },
  "next": {
    /* Même structure pour l'émission suivante */
  }
}
```

### Champs importants

- **`delayToRefresh`** : Temps en millisecondes avant le prochain rafraîchissement recommandé
- **`now.song`** : Présent si c'est une musique (FIP, France Musique)
- **`now.media.sources`** : Liste de tous les flux disponibles avec formats et bitrates
- **`localRadios`** : Liste des radios locales (pour France Bleu)

---

## 2. API LiveMeta (ancienne API, toujours fonctionnelle)

### Format
```
https://api.radiofrance.fr/livemeta/live/{id}/transistor_{station}_player
```

### IDs connus

| Station | ID | Endpoint |
|---------|-----|----------|
| France Culture | 5 | `/livemeta/live/5/transistor_culture_player` |

### Exemple de réponse

```json
{
  "prev": [{
    "firstLine": "Le direct",
    "secondLine": "France Culture, l'esprit d'ouverture",
    "cover": "uuid-image",
    "startTime": null,
    "endTime": null
  }],
  "now": {
    "firstLine": "La Série fiction",
    "firstLineUuid": "uuid",
    "firstLinePath": "franceculture/podcasts/emission",
    "secondLine": "Titre de l'épisode",
    "cover": "uuid-image",
    "startTime": 1769108400,
    "endTime": 1769110122
  },
  "next": [{ /* émission suivante */ }],
  "delayToRefresh": 742000
}
```

**Note :** Cette API retourne moins de détails que `/api/live?` mais fonctionne toujours.

---

## 3. Flux audio

### Format des URLs

#### HLS (recommandé)
```
https://stream.radiofrance.fr/{station}/{station}.m3u8?id=radiofrance
```

#### Icecast (AAC et MP3)
```
https://icecast.radiofrance.fr/{station}-{qualite}.{format}?id=radiofrance
```

### Qualités disponibles

| Qualité | Bitrate AAC | Bitrate MP3 | Utilisation |
|---------|-------------|-------------|-------------|
| `lofi` | 32 kbps | 32 kbps | Connexions lentes |
| `midfi` | 128 kbps | 128 kbps | Standard |
| `hifi` | 192 kbps | - | Haute qualité |

### Exemples d'URLs

**France Culture :**
```
https://stream.radiofrance.fr/franceculture/franceculture.m3u8?id=radiofrance
https://icecast.radiofrance.fr/franceculture-hifi.aac?id=radiofrance
https://icecast.radiofrance.fr/franceculture-midfi.aac?id=radiofrance
https://icecast.radiofrance.fr/franceculture-midfi.mp3?id=radiofrance
https://icecast.radiofrance.fr/franceculture-lofi.aac?id=radiofrance
https://icecast.radiofrance.fr/franceculture-lofi.mp3?id=radiofrance
```

**France Inter :**
```
https://stream.radiofrance.fr/franceinter/franceinter.m3u8?id=radiofrance
https://icecast.radiofrance.fr/franceinter-hifi.aac?id=radiofrance
https://icecast.radiofrance.fr/franceinter-midfi.aac?id=radiofrance
https://icecast.radiofrance.fr/franceinter-midfi.mp3?id=radiofrance
https://icecast.radiofrance.fr/franceinter-lofi.aac?id=radiofrance
https://icecast.radiofrance.fr/franceinter-lofi.mp3?id=radiofrance
```

---

## 4. Webradios thématiques

### FIP Webradios

FIP propose plusieurs webradios thématiques. Format des URLs :

```
https://icecast.radiofrance.fr/fip{variant}-{qualite}.aac?id=radiofrance
```

#### Variantes disponibles (confirmées)

| Variante | URL | Status |
|----------|-----|--------|
| FIP principale | `fip-hifi.aac` | ✅ |
| FIP Rock | `fiprock-hifi.aac` | ✅ |
| FIP Jazz | `fipjazz-hifi.aac` | ✅ |
| FIP Groove | `fipgroove-hifi.aac` | ✅ |
| FIP Reggae | `fipreggae-hifi.aac` | ✅ |
| FIP Electro | `fipelectro-hifi.aac` | ✅ |
| FIP Metal | `fipmetal-hifi.aac` | ✅ |
| FIP Nouveautés | `fipnouveautes-hifi.aac` | ✅ |
| FIP Pop | `fippop-hifi.aac` | ✅ |

**Exemples :**
```
https://icecast.radiofrance.fr/fiprock-hifi.aac?id=radiofrance
https://icecast.radiofrance.fr/fipjazz-midfi.aac?id=radiofrance
https://icecast.radiofrance.fr/fipgroove-lofi.aac?id=radiofrance
```

### France Musique Webradios

Format similaire :

```
https://icecast.radiofrance.fr/francemusique{variant}-{qualite}.aac?id=radiofrance
```

#### Variantes disponibles (confirmées)

| Variante | URL | Status |
|----------|-----|--------|
| France Musique principale | `francemusique-hifi.aac` | ✅ |
| La Jazz | `francemusiquelajazz-hifi.aac` | ✅ |
| La Contemporaine | `francemusiquelacontemporaine-hifi.aac` | ✅ |
| Baroque | `francemusiquebaroque-hifi.aac` | ✅ |
| Opéra | `francemusiqueopera-hifi.aac` | ✅ |

**Exemples :**
```
https://icecast.radiofrance.fr/francemusiquelajazz-hifi.aac?id=radiofrance
https://icecast.radiofrance.fr/francemusiquebaroque-midfi.aac?id=radiofrance
```

---

## 5. France Bleu - Radios locales

### API
```
https://www.radiofrance.fr/francebleu/api/live?
```

### Structure spécifique

Le champ `localRadios` contient la liste de toutes les radios locales :

```json
{
  "stationName": "francebleu",
  "delayToRefresh": 2090000,
  "now": { /* ... */ },
  "localRadios": [
    {
      "id": 12,
      "title": "ICI Alsace",
      "name": "francebleu_alsace",
      "isOnAir": true
    },
    {
      "id": 13,
      "title": "ICI Armorique",
      "name": "francebleu_armorique",
      "isOnAir": true
    }
    // ... ~40 radios locales
  ]
}
```

### Format des flux locaux

**Hypothèse (à confirmer) :**
```
https://icecast.radiofrance.fr/fb{nom}-hifi.aac?id=radiofrance
```

Exemple :
```
https://icecast.radiofrance.fr/fbalsace-hifi.aac?id=radiofrance
```

---

## 6. API Pikapi (Images)

### Format
```
https://www.radiofrance.fr/pikapi/images/{uuid}/{taille}
```

### Tailles disponibles

Basé sur l'analyse des réponses, plusieurs tailles semblent disponibles :

- `88x88` - Miniature
- `200x200` - Petite
- `420x720` - Moyenne portrait
- `560x960` - Grande portrait
- `1200x680` - Grande paysage
- `raw` - Taille originale

**Exemples :**
```
https://www.radiofrance.fr/pikapi/images/436430f7-5b2b-43f2-9f3c-28f2ad6cae39/200x200
https://www.radiofrance.fr/pikapi/images/436430f7-5b2b-43f2-9f3c-28f2ad6cae39/1200x680
```

---

## 7. Autres endpoints (configuration)

D'après l'analyse du fichier HAR, voici d'autres endpoints internes utilisés :

### Endpoints de configuration (dans `__data.json`)

- **`https://kirby.radiofrance.fr`** - CMS Kirby
- **`https://www.radiofrance.fr/pikapi`** - API images
- **`https://www.radiofrance.fr/transistor`** - API Transistor
- **`https://api.radiofrance.fr/livemeta/live`** - API LiveMeta
- **`https://preroll.radiofrance.fr`** - Publicités pre-roll

### API Expressions (contenu éditorial)

```
https://www.radiofrance.fr/api/expressions?variant=vertical&limit=36&ids={uuid,uuid,...}
```

Retourne des contenus éditoriaux par UUIDs.

---

## 8. Résumé pour PMOMusic

### Recommandations d'implémentation

#### Pour les métadonnées live

**Option 1 (recommandée) :** API `/api/live?` par station
```rust
async fn fetch_live_metadata(station: &str) -> Result<LiveMetadata> {
    let url = format!("https://www.radiofrance.fr/{}/api/live?", station);
    reqwest::get(&url).await?.json().await
}
```

**Avantages :**
- ✅ Données complètes (émission, producteur, intro, visuels)
- ✅ Flux audio inclus dans la réponse
- ✅ `delayToRefresh` pour polling intelligent
- ✅ Support des radios locales (France Bleu)

#### Pour les flux audio

**Priorisation recommandée :**

1. **HLS** (format moderne, adaptatif)
2. **AAC hifi** (192 kbps, meilleure qualité)
3. **AAC midfi** (128 kbps, bon compromis)
4. **MP3 midfi** (128 kbps, compatibilité maximale)
5. **AAC/MP3 lofi** (32 kbps, fallback)

#### Polling intelligent

Utiliser le champ `delayToRefresh` pour optimiser :

```rust
loop {
    let metadata = fetch_live_metadata("franceculture").await?;
    
    // Afficher/utiliser les métadonnées
    println!("{} - {}", 
        metadata.now.first_line.title,
        metadata.now.second_line.title
    );
    
    // Attendre le temps recommandé
    tokio::time::sleep(
        Duration::from_millis(metadata.delay_to_refresh)
    ).await;
}
```

### Liste complète des stations à supporter

**Stations principales :**
- France Inter
- France Info
- France Culture
- France Musique
- FIP
- Mouv'
- Mon Petit France Inter

**Webradios FIP (9):**
- FIP principale
- FIP Rock, Jazz, Groove, Reggae, Electro, Metal, Nouveautés, Pop

**Webradios France Musique (5+):**
- France Musique principale
- La Jazz, La Contemporaine, Baroque, Opéra

**Radios locales France Bleu (~40):**
- À récupérer dynamiquement via `/francebleu/api/live?`

---

## 9. Points d'attention

### Rate limiting
- Pas de limite documentée observée
- Utiliser `delayToRefresh` pour respecter les recommandations
- Éviter les requêtes inutiles (cache local)

### User-Agent
Pour un projet open-source, utiliser un User-Agent identifiable :
```
PMOMusic/0.3.10 (https://github.com/votre-repo)
```

### Gestion d'erreurs
- Les APIs peuvent retourner des données vides (`null`)
- Le champ `song` n'existe que pour les radios musicales
- `localRadios` n'existe que pour France Bleu

### Respect des CGU
- Ces APIs sont utilisées par le site officiel
- Usage pour un projet open-source personnel/non-commercial
- Ne pas redistribuer les flux audio commercialement

---

## 10. Annexes

### Exemple complet en Rust

```rust
use serde::{Deserialize, Serialize};
use reqwest;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveResponse {
    pub station_name: String,
    pub delay_to_refresh: u64,
    pub migrated: bool,
    pub now: ShowMetadata,
    pub next: Option<ShowMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowMetadata {
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    pub producer: Option<String>,
    pub first_line: Line,
    pub second_line: Line,
    pub third_line: Option<Line>,
    pub intro: Option<String>,
    pub song: Option<Song>,
    pub media: Media,
}

#[derive(Debug, Deserialize)]
pub struct Line {
    pub title: Option<String>,
    pub id: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Song {
    pub id: String,
    pub year: Option<u32>,
    pub interpreters: Vec<String>,
    pub release: Release,
}

#[derive(Debug, Deserialize)]
pub struct Release {
    pub label: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Media {
    pub sources: Vec<Source>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub url: String,
    pub broadcast_type: String,
    pub format: String,
    pub bitrate: u32,
}

pub async fn get_live_metadata(station: &str) -> Result<LiveResponse, reqwest::Error> {
    let url = format!("https://www.radiofrance.fr/{}/api/live?", station);
    
    reqwest::Client::new()
        .get(&url)
        .header("User-Agent", "PMOMusic/0.3.10")
        .send()
        .await?
        .json()
        .await
}
```

### Stations complètes testées

| Station | API Live | Flux HLS | Flux AAC | Flux MP3 |
|---------|----------|----------|----------|----------|
| France Inter | ✅ | ✅ | ✅ | ✅ |
| France Info | ✅ | ✅ | ✅ | ✅ |
| France Culture | ✅ | ✅ | ✅ | ✅ |
| France Musique | ✅ | ✅ | ✅ | ✅ |
| FIP | ✅ | ✅ | ✅ | ✅ |
| Mouv' | ✅ | ✅ | ✅ | ✅ |
| France Bleu | ✅ | ✅ | ✅ | ✅ |
| Mon Petit France Inter | ✅ | ✅ (à tester) | ✅ (à tester) | ✅ (à tester) |

---

**Dernière mise à jour :** 2026-01-22  
**Méthode d'analyse :** Capture HAR + tests directs des endpoints  
**Statut :** Toutes les APIs sont publiques et fonctionnelles sans authentification

---

# Round 4 : Architecture Client Stateful pour PMORadioFrance

## Date : 2026-01-22

## Contexte

Le Round 3 a permis d'implémenter un client HTTP basique (`RadioFranceClient`) pour interroger l'API Radio France. Ce client est **stateless** : il ne gère pas de cache, ne maintient pas d'état, et doit interroger l'API à chaque requête.

Le Round 4 vise à construire la couche suivante : un **client stateful** qui :
- Gère un cache des stations découvertes avec TTL
- Expose des méthodes de haut niveau pour obtenir des listes de radios
- Prépare les données pour la construction de la source UPnP
- Intègre avec pmoconfig pour stocker les informations persistantes

## Objectifs du client stateful

### 1. Cache intelligent des stations

**Problématique** : La découverte de toutes les stations (méthode `discover_all_stations()`) fait ~10 requêtes HTTP et prend 3-5 secondes. Les stations Radio France ne changent que très rarement (nouvelles webradios ~1-2 fois par an, nouvelles stations locales jamais).

**Solution** : Utiliser pmoconfig pour stocker la liste des stations avec un timestamp, et ne rafraîchir que si le TTL est dépassé (ou sur requête forcée).

**Stratégie de cache** :
```yaml
# Dans .pmomusic/config.yaml
sources:
  radiofrance:
    stations_cache:
      version: 1                    # Version du schéma de découverte
      last_updated: 1737565200      # Unix timestamp
      ttl_days: 7                   # TTL par défaut : 7 jours
      stations:
        - slug: "franceculture"
          name: "France Culture"
          type: "main"
        - slug: "fip"
          name: "FIP"
          type: "main"
        - slug: "fip_rock"
          name: "FIP Rock"
          type: "webradio"
          parent: "fip"
        - slug: "francebleu_alsace"
          name: "ICI Alsace"
          type: "local"
          region: "Alsace"
          id: 12
        # ... ~50+ stations au total
```

**Logique de rafraîchissement** :
1. Lire le cache depuis la config
2. Vérifier `version` (invalide si ancienne version de découverte)
3. Vérifier TTL : `now - last_updated < ttl_days * 86400`
4. Si valide : retourner le cache
5. Si invalide ou absent : appeler `discover_all_stations()` et mettre à jour la config

### 2. Organisation des stations

Les stations doivent être organisées logiquement pour la navigation UPnP :

```
Radio France (racine)
├── France Culture
├── France Inter
├── France Info
├── France Musique
├── FIP
│   ├── FIP (principale)
│   ├── FIP Rock
│   ├── FIP Jazz
│   ├── FIP Groove
│   ├── FIP Reggae
│   ├── FIP Electro
│   ├── FIP Metal
│   ├── FIP Nouveautés
│   └── FIP Pop
├── Mouv'
└── ICI (France Bleu renommé)
    ├── ICI Alsace
    ├── ICI Armorique
    ├── ICI Auxerre
    ├── ... (~40 radios locales)
    └── ICI Vaucluse
```

**Règles de regroupement** :
- **Stations principales** : Une entrée par station principale (France Culture, France Inter, etc.)
- **Stations avec webradios** (FIP, France Musique) : Un folder contenant :
  1. La station principale en premier
  2. Les webradios triées alphabétiquement
- **France Bleu** : Renommé "ICI" avec toutes les radios locales dedans

**Changement de label** :
- API retourne : `"France Bleu"` → Affichage : `"ICI"`
- API retourne : `"ICI Alsace"` → Affichage : `"ICI Alsace"` (inchangé)
- Slugs conservés tels quels : `francebleu_alsace`, `francebleu`, etc.

### 3. Métadonnées live avec rafraîchissement intelligent

Pour chaque station, on doit pouvoir obtenir les métadonnées live avec cache court terme :

**Cache de métadonnées live** :
- Durée : Utiliser le champ `delayToRefresh` de l'API (généralement 2-5 minutes)
- Stockage : En mémoire uniquement (pas dans pmoconfig)
- Invalidation : Automatique après `delayToRefresh` millisecondes

**Stratégie** :
```rust
struct LiveMetadataCache {
    metadata: LiveResponse,
    fetched_at: SystemTime,
    valid_until: SystemTime,
}

// Pseudo-code
fn get_live_metadata(station: &str) -> Result<LiveResponse> {
    if let Some(cached) = memory_cache.get(station) {
        if SystemTime::now() < cached.valid_until {
            return Ok(cached.metadata.clone());
        }
    }
    
    let metadata = client.live_metadata(station).await?;
    let delay = Duration::from_millis(metadata.delay_to_refresh);
    
    memory_cache.insert(station, LiveMetadataCache {
        metadata: metadata.clone(),
        fetched_at: SystemTime::now(),
        valid_until: SystemTime::now() + delay,
    });
    
    Ok(metadata)
}
```

### 4. Construction de playlists pour la source UPnP

Chaque station doit être exposée comme une **playlist volatile** contenant un seul item : le stream de plus haute qualité.

**Règles métier pour les playlists** :

#### Format des playlists

```rust
// Pseudo-structure d'une playlist de station
PMOPlaylist {
    id: "radiofrance:franceculture",
    role: PlaylistRole::Radio,
    volatile: true,  // Les métadonnées changent, pas le contenu
    
    // Métadonnées de la playlist (= métadonnées de la station)
    title: "France Culture",  // Nom de la station
    artist: "Les Matins",     // Nom de l'émission en cours (now.firstLine.title)
    album: "France Culture",  // Nom de la station (répété)
    cover_pk: "COVER_PK",     // Cover de l'émission en cours (now.visualBackground)
    
    // Contenu : UN SEUL ITEM
    items: [
        PMOItem {
            id: "radiofrance:franceculture:stream",
            title: "Le Journal de l'éco • Le jouet profite...",  // now.secondLine.title
            artist: "Guillaume Erner",     // now.producer ou show producer
            album: "Les Matins",            // now.firstLine.title (émission)
            genre: "Talk Radio",            // Type de station
            
            // Stream URL (AAC 192 kbps ou HLS)
            url: "https://icecast.radiofrance.fr/franceculture-hifi.aac?id=radiofrance",
            
            // Métadonnées techniques
            protocol_info: "http-get:*:audio/aac:*",
            bitrate: 192000,
            sample_rate: 48000,
            channels: 2,
            
            // Cover de l'émission/morceau
            cover_pk: "COVER_PK",
        }
    ]
}
```

#### Mapping des métadonnées API → UPnP

**Pour les radios parlées (France Culture, France Inter, France Info)** :

| Champ UPnP | Source API | Exemple |
|------------|------------|---------|
| Playlist Title | Station name | "France Culture" |
| Playlist Artist | `now.firstLine.title` | "Les Matins" |
| Playlist Cover | `now.visualBackground` → cache | UUID de cover |
| Item Title | `now.firstLine.title` + `now.secondLine.title` | "Les Matins • Le Journal de l'éco" |
| Item Artist | `now.producer` | "Guillaume Erner" |
| Item Album | `now.firstLine.title` | "Les Matins" |
| Item Cover | `now.visualBackground` → cache | UUID de cover |
| Item Genre | "Talk Radio" | Fixe |

**Pour les radios musicales (FIP, France Musique)** :

| Champ UPnP | Source API | Exemple |
|------------|------------|---------|
| Playlist Title | Station name | "FIP Rock" |
| Playlist Artist | `now.song.artists` OU `now.firstLine.title` | "The Rolling Stones" |
| Playlist Cover | `now.song` image OU `now.visualBackground` | UUID de cover |
| Item Title | `now.song.title` OU `now.firstLine.title` | "Paint It Black" |
| Item Artist | `now.song.artists` | "The Rolling Stones" |
| Item Album | `now.song.release.title` | "Aftermath" |
| Item Cover | `now.song` image OU `now.visualBackground` | UUID de cover |
| Item Genre | "Music" OU genre spécifique | "Rock" |

**Note importante** : Les métadonnées changent régulièrement (toutes les 2-5 minutes), mais l'URL du stream reste la même. C'est pour cela que les playlists sont **volatiles** : on ne change pas leur contenu (toujours 1 item), mais on met à jour les métadonnées de cet item.

#### Gestion des covers

**Stratégie de cache** :
- Les covers doivent être cachées dans `pmocovers`
- URL source : `now.visualBackground.src` ou image de `now.song`
- Extraction UUID : Parser l'URL Pikapi pour extraire l'UUID
- Transformation : Télécharger et convertir en WebP si nécessaire
- Stockage : Cache avec le PK = `RADIOFRANCE:{uuid}`

**Workflow de cache de cover** :
```rust
// Pseudo-code
async fn cache_cover_from_metadata(metadata: &ShowMetadata) -> Option<String> {
    // 1. Extraire l'URL de l'image
    let image_url = metadata.visual_background.as_ref()?.src.clone();
    
    // 2. Extraire UUID
    let uuid = extract_uuid_from_url(&image_url)?;
    
    // 3. Construire URL en haute résolution
    let hires_url = ImageSize::XLarge.build_url(&uuid);
    
    // 4. Cacher avec pmocovers
    let cover_pk = cache_manager.cache_cover(&hires_url).await.ok()?;
    
    Some(cover_pk)
}
```

**Tailles de cover** :
- Pour les métadonnées UPnP : Utiliser `ImageSize::XLarge` (1200x680) ou `ImageSize::Large` (560x960)
- Pikapi supporte plusieurs tailles, on choisit la plus grande disponible

#### URL des streams

**Règle métier** : Ne présenter que le stream de **plus haute résolution** disponible.

**Priorité de sélection** :
1. AAC 192 kbps (HiFi) : `https://icecast.radiofrance.fr/{station}-hifi.aac?id=radiofrance`
2. HLS adaptatif : `https://stream.radiofrance.fr/{station}/{station}.m3u8?id=radiofrance`
3. AAC 128 kbps (MidFi) : Fallback si HiFi indisponible
4. MP3 128 kbps : Fallback ultime

**Pas de cache audio** : Les streams sont des flux en direct, on ne les cache JAMAIS dans `pmoaudiocache`. Les URLs sont passées telles quelles au renderer.

### 5. Interface du client stateful

**Proposition d'API publique** :

```rust
/// Client stateful pour Radio France avec cache et gestion d'état
pub struct RadioFranceStatefulClient {
    client: RadioFranceClient,          // Client HTTP basique
    config: Arc<Config>,                 // Configuration pmoconfig
    metadata_cache: Arc<RwLock<HashMap<String, LiveMetadataCache>>>,
    cache_manager: SourceCacheManager,   // Pour covers
}

impl RadioFranceStatefulClient {
    /// Créer un nouveau client stateful
    pub async fn new() -> Result<Self>;
    
    /// Créer avec un client HTTP personnalisé
    pub fn with_client_and_config(
        client: RadioFranceClient,
        config: Arc<Config>,
    ) -> Self;
    
    // ========================================================================
    // Station Discovery (avec cache)
    // ========================================================================
    
    /// Obtenir toutes les stations (depuis cache si valide, sinon découverte)
    pub async fn get_all_stations(&self) -> Result<Vec<Station>>;
    
    /// Forcer la redécouverte des stations (ignore le cache)
    pub async fn refresh_stations(&self) -> Result<Vec<Station>>;
    
    /// Obtenir les stations principales uniquement
    pub async fn get_main_stations(&self) -> Result<Vec<Station>>;
    
    /// Obtenir les webradios d'une station (ex: FIP Rock, FIP Jazz)
    pub async fn get_webradios(&self, parent_station: &str) -> Result<Vec<Station>>;
    
    /// Obtenir les radios locales ICI (France Bleu)
    pub async fn get_local_radios(&self) -> Result<Vec<Station>>;
    
    // ========================================================================
    // Organisation hiérarchique
    // ========================================================================
    
    /// Obtenir les stations organisées par groupe
    pub async fn get_stations_by_group(&self) -> Result<StationGroups>;
    
    // ========================================================================
    // Métadonnées live (avec cache court terme)
    // ========================================================================
    
    /// Obtenir les métadonnées live d'une station (cache 2-5 min)
    pub async fn get_live_metadata(&self, station: &str) -> Result<LiveResponse>;
    
    /// Forcer le rafraîchissement des métadonnées (ignore le cache)
    pub async fn refresh_live_metadata(&self, station: &str) -> Result<LiveResponse>;
    
    // ========================================================================
    // Construction de playlists
    // ========================================================================
    
    /// Construire une playlist UPnP pour une station
    pub async fn build_station_playlist(&self, station: &str) -> Result<StationPlaylist>;
    
    /// Mettre à jour les métadonnées d'une playlist existante
    pub async fn update_playlist_metadata(
        &self,
        station: &str,
        playlist: &mut StationPlaylist,
    ) -> Result<()>;
    
    // ========================================================================
    // Helpers
    // ========================================================================
    
    /// Obtenir l'URL du stream HiFi pour une station
    pub async fn get_stream_url(&self, station: &str) -> Result<String>;
    
    /// Vérifier si le cache des stations est valide
    pub fn is_station_cache_valid(&self) -> bool;
    
    /// Obtenir l'âge du cache des stations (en secondes)
    pub fn station_cache_age_secs(&self) -> Option<u64>;
}

/// Groupes de stations organisés hiérarchiquement
pub struct StationGroups {
    /// Stations principales sans webradios (France Culture, France Inter, etc.)
    pub standalone: Vec<Station>,
    
    /// Stations avec webradios (FIP, France Musique)
    pub with_webradios: Vec<StationGroup>,
    
    /// Radios locales ICI (France Bleu)
    pub local_radios: Vec<Station>,
}

/// Groupe de stations (principale + webradios)
pub struct StationGroup {
    /// Station principale
    pub main: Station,
    
    /// Webradios associées (triées alphabétiquement)
    pub webradios: Vec<Station>,
}

/// Playlist UPnP pour une station
pub struct StationPlaylist {
    /// ID de la playlist
    pub id: String,
    
    /// Station source
    pub station: Station,
    
    /// Métadonnées de la playlist (changent avec les émissions)
    pub metadata: PlaylistMetadata,
    
    /// Item unique (stream)
    pub stream_item: StreamItem,
}

/// Métadonnées de playlist (volatiles)
pub struct PlaylistMetadata {
    pub title: String,          // Nom de la station
    pub artist: Option<String>, // Émission en cours
    pub album: Option<String>,  // Nom de la station (répété)
    pub cover_pk: Option<String>, // Cover cachée
}

/// Item de stream
pub struct StreamItem {
    pub id: String,
    pub title: String,          // Titre de l'émission/morceau
    pub artist: Option<String>, // Producteur/artiste
    pub album: Option<String>,  // Nom de l'émission/album
    pub genre: Option<String>,
    pub url: String,            // URL du stream (AAC HiFi ou HLS)
    pub protocol_info: String,
    pub bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub cover_pk: Option<String>,
}
```

### 6. Extension de configuration (config_ext.rs)

**Trait d'extension pour pmoconfig** :

```rust
pub trait RadioFranceConfigExt {
    // ========================================================================
    // Activation de la source
    // ========================================================================
    
    fn get_radiofrance_enabled(&self) -> Result<bool>;
    fn set_radiofrance_enabled(&self, enabled: bool) -> Result<()>;
    
    // ========================================================================
    // Cache des stations
    // ========================================================================
    
    fn get_radiofrance_stations_cache(&self) -> Result<Option<CachedStationList>>;
    fn set_radiofrance_stations_cache(&self, cache: &CachedStationList) -> Result<()>;
    fn clear_radiofrance_stations_cache(&self) -> Result<()>;
    
    fn get_radiofrance_cache_ttl_days(&self) -> Result<u64>;
    fn set_radiofrance_cache_ttl_days(&self, days: u64) -> Result<()>;
    
    // ========================================================================
    // Configuration client HTTP
    // ========================================================================
    
    fn get_radiofrance_base_url(&self) -> Result<String>;
    fn set_radiofrance_base_url(&self, url: String) -> Result<()>;
    
    fn get_radiofrance_timeout_secs(&self) -> Result<u64>;
    fn set_radiofrance_timeout_secs(&self, secs: u64) -> Result<()>;
    
    // ========================================================================
    // Factory method
    // ========================================================================
    
    fn create_radiofrance_client(&self) -> Result<RadioFranceStatefulClient>;
}
```

**Chemins de configuration** :

```yaml
sources:
  radiofrance:
    enabled: true                 # Activation de la source
    base_url: "https://www.radiofrance.fr"
    timeout_secs: 30
    cache_ttl_days: 7            # TTL du cache des stations
    
    stations_cache:              # Cache des stations découvertes
      version: 1
      last_updated: 1737565200
      stations:
        - slug: "franceculture"
          name: "France Culture"
          type: "main"
        # ... reste des stations
```

## Workflow de mise à jour des métadonnées

### Scénario 1 : Première utilisation

1. Utilisateur ouvre la source Radio France dans son client UPnP
2. `RadioFranceSource::browse("radiofrance")` est appelé
3. Source appelle `stateful_client.get_all_stations()`
4. Cache vide → Appel `discover_all_stations()` (~3-5 secondes)
5. Résultat stocké dans config avec timestamp
6. Retour de la liste des stations

### Scénario 2 : Utilisation ultérieure (cache valide)

1. Utilisateur ouvre la source Radio France
2. Source appelle `stateful_client.get_all_stations()`
3. Cache présent et valide (< 7 jours) → Retour immédiat depuis config
4. Pas d'appel réseau

### Scénario 3 : Lecture d'une station

1. Utilisateur sélectionne "France Culture" et lance la lecture
2. Source appelle `stateful_client.build_station_playlist("franceculture")`
3. Stateful client :
   - Appelle `get_live_metadata("franceculture")` (cache 2-5 min si présent)
   - Extrait les métadonnées de l'émission en cours
   - Cache la cover de l'émission via `pmocovers`
   - Construit la playlist avec 1 item (stream HiFi)
4. Retour de la playlist au renderer

### Scénario 4 : Mise à jour des métadonnées pendant la lecture

1. Renderer lit le stream depuis 3 minutes
2. Control point demande les métadonnées à jour
3. Source appelle `stateful_client.update_playlist_metadata()`
4. Stateful client :
   - Vérifie le cache des métadonnées live
   - Si expiré (> `delayToRefresh` ms) : appelle l'API
   - Met à jour les métadonnées de la playlist
   - Cache la nouvelle cover si différente
5. Control point reçoit les nouvelles métadonnées

**Important** : L'URL du stream ne change JAMAIS pendant la lecture. Seules les métadonnées (titre, artiste, cover) changent.

## Architecture des fichiers

```
pmoradiofrance/
├── src/
│   ├── lib.rs                   # Exports publics
│   ├── client.rs                # Client HTTP basique (Round 3) ✅
│   ├── models.rs                # Structures de données (Round 3) ✅
│   ├── error.rs                 # Types d'erreur ✅
│   ├── stateful_client.rs       # Client stateful (Round 4) 🆕
│   ├── playlist.rs              # Construction de playlists (Round 4) 🆕
│   ├── config_ext.rs            # Extension pmoconfig (Round 4) 🆕
│   └── source.rs                # Implémentation MusicSource (Round 5)
├── assets/
│   └── default.webp             # Logo Radio France 300x300px
├── Cargo.toml
└── README.md
```

## Dépendances supplémentaires

```toml
[dependencies]
# Déjà présentes (Round 3)
reqwest = { version = "0.12", features = ["json"] }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
serde_yaml = { workspace = true }
chrono = { workspace = true }
async-trait = { workspace = true }
thiserror = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
url = "2.5"
scraper = "0.22"
regex = "1.11"
pmosource = { path = "../pmosource" }

# Nouvelles (Round 4)
pmoconfig = { path = "../pmoconfig" }      # Configuration persistante
pmocovers = { path = "../pmocovers" }      # Cache de covers
# pmoaudiocache NON utilisé (pas de cache audio pour les streams live)

[features]
default = ["pmoconfig"]
pmoconfig = ["dep:pmoconfig"]
cache = ["dep:pmocovers"]
logging = []
server = ["pmosource/server", "pmoconfig", "cache"]
full = ["server", "logging"]
```

## Considérations d'implémentation

### Thread safety

Le client stateful doit être thread-safe car il sera partagé entre plusieurs threads (ContentDirectory, AVTransport, etc.) :

```rust
pub struct RadioFranceStatefulClient {
    client: RadioFranceClient,                    // Clone cheap (Arc interne)
    config: Arc<Config>,                          // Partagé
    metadata_cache: Arc<RwLock<HashMap<...>>>,   // Cache mémoire protégé
    cache_manager: SourceCacheManager,            // Thread-safe
}

impl Clone for RadioFranceStatefulClient {
    fn clone(&self) -> Self {
        // Clone cheap : tous les champs sont Arc ou Clone
        Self {
            client: self.client.clone(),
            config: self.config.clone(),
            metadata_cache: self.metadata_cache.clone(),
            cache_manager: self.cache_manager.clone(),
        }
    }
}
```

### Performances

**Cache des stations** :
- Stockage : YAML dans config (~10-20 KB pour ~50 stations)
- Lecture : Désérialisation YAML (~1-2 ms)
- TTL : 7 jours (configurable)

**Cache des métadonnées live** :
- Stockage : Mémoire (HashMap)
- Taille : ~5-10 KB par station
- TTL : 2-5 minutes (champ `delayToRefresh` de l'API)
- Limite : ~100 stations max = ~1 MB max

**Cache des covers** :
- Via `pmocovers` (LRU disk cache)
- Taille moyenne : 50-200 KB par cover WebP
- Limite : Configurable via `pmocovers` (défaut : 2000 items)

### Gestion d'erreurs

**Stratégie de fallback** :

1. **Cache des stations invalide ou absent** → Redécouverte (erreur propagée si échec)
2. **Métadonnées live indisponibles** → Utiliser cache expiré si présent, sinon erreur
3. **Cover indisponible** → Utiliser cover par défaut de la source
4. **Stream HiFi indisponible** → Fallback sur HLS puis AAC MidFi

### Logging

Utiliser `tracing` pour logger :
- Découverte des stations (nombre, durée)
- Hits/miss du cache
- Rafraîchissement des métadonnées
- Erreurs réseau

## Tests

### Tests unitaires

- Validation du cache (TTL, version, invalidation)
- Parsing des métadonnées
- Construction des playlists
- Mapping API → UPnP

### Tests d'intégration

- Découverte réelle des stations
- Récupération des métadonnées live
- Cache et invalidation
- Construction de playlists complètes

## Prochaines étapes (Round 5)

Le Round 5 implémentera la `MusicSource` finale qui :
- Utilise le `RadioFranceStatefulClient`
- Implémente le trait `MusicSource` de `pmosource`
- Expose l'arborescence UPnP ContentDirectory
- Gère les playlists volatiles via `pmoplaylist`
- Notifie les changements de métadonnées

---

**Fin du Round 4**
