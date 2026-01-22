# Architecture du client Radio France (client.rs)

**Date** : 2026-01-22  
**Objectif** : Conception d'une API Rust pour interroger les métadonnées live et flux audio de Radio France  
**Référence** : Architecture inspirée de `pmoparadise/src/client.rs`

---

## Table des matières

1. [Vue d'ensemble](#vue-densemble)
2. [Découverte dynamique des stations](#découverte-dynamique-des-stations)
3. [Architecture du client](#architecture-du-client)
4. [Structures de données](#structures-de-données)
5. [Méthodes principales](#méthodes-principales)
6. [Exemple d'utilisation](#exemple-dutilisation)
7. [Points d'attention](#points-dattention)

---

## Vue d'ensemble

Le client Radio France doit permettre :
- **Découverte dynamique** de ~73 stations/webradios (scraping HTML)
- **Métadonnées live** via `/api/live?` avec polling intelligent
- **Flux audio** en qualité maximale uniquement (AAC 192 kbps + HLS)
- **Un seul client** pour toutes les stations (pas un client par station)

### Philosophie

- **Pas de hardcoding** : Toutes les stations sont découvertes dynamiquement
- **Qualité maximale uniquement** : AAC 192 kbps (hifi) + HLS, pas de choix lofi/midfi
- **Architecture simple** : Un client unique, les stations sont des paramètres

---

## Découverte dynamique des stations

### Stratégie complète

Radio France n'expose **pas d'API centralisée** listant toutes les stations. La découverte se fait par **scraping HTML** des pages principales :

#### 1. Stations principales (8)

**Source** : `https://www.radiofrance.fr/`

**Méthode** : Scraper le HTML et extraire tous les slugs via regex `(franceinter|franceinfo|franceculture|francemusique|fip|mouv|francebleu|monpetit)`

**Résultat attendu** :
```
franceinter
franceinfo
franceculture
francemusique
fip
mouv
francebleu
monpetitfranceinter
```

#### 2. Webradios de chaque station (nombre variable)

**Principe** : **TOUTES les stations** peuvent avoir des webradios, pas seulement FIP et France Musique.

**Méthode** : Pour chaque station principale découverte, scraper sa page `https://www.radiofrance.fr/{station}` et extraire les identifiants via regex `{station}_[a-z_]+`

**Exemples découverts** :

**FIP** (`https://www.radiofrance.fr/fip`) :
```
fip_cultes
fip_electro
fip_groove
fip_hiphop
fip_jazz
fip_metal
fip_nouveautes
fip_pop
fip_reggae
fip_rock
fip_sacre_francais
fip_world
```

**France Musique** (`https://www.radiofrance.fr/francemusique`) :
```
francemusique_baroque
francemusique_classique_easy
francemusique_classique_love
francemusique_classique_plus
francemusique_concert_rf
francemusique_evenementielle
francemusique_la_contemporaine
francemusique_la_jazz
francemusique_ocora_monde
francemusique_opera
francemusique_piano_zen
```

**Autres stations** : À découvrir dynamiquement (France Inter, Mouv, etc. pourraient avoir des webradios futures)

#### 3. Radios locales France Bleu (~40)

**Source** : API `/francebleu/api/live?` → champ `localRadios[]`

**Méthode** : Appel API et extraction du tableau JSON

**Exemple de structure** :
```json
{
  "localRadios": [
    {"id": 12, "title": "ICI Alsace", "name": "francebleu_alsace", "isOnAir": true},
    {"id": 13, "title": "ICI Armorique", "name": "francebleu_armorique", "isOnAir": true},
    ...
  ]
}
```

### Total découvert

- **8** stations principales
- **~23** webradios (12 FIP + 11 France Musique + possibles autres)
- **~40** radios locales France Bleu
- **= ~71+ stations au total** (extensible automatiquement si nouvelles webradios)

---

## Architecture du client

### Client unique

Contrairement à une approche "un client par station", nous utilisons **un seul client** avec les stations comme **paramètres de méthode**.

```rust
pub struct RadioFranceClient {
    client: reqwest::Client,
    timeout: Duration,
}
```

### Pas de cache interne

Le client est **stateless** et ne cache rien. La gestion du cache (métadonnées, images) sera faite par les couches supérieures (`SourceCacheManager`).

### Builder pattern

Pour permettre la configuration :

```rust
pub struct ClientBuilder {
    client: Option<reqwest::Client>,
    timeout: Duration,
    user_agent: String,
}
```

---

## Structures de données

### 1. Station découverte

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub slug: String,              // "fip_rock", "franceinter"
    pub name: String,              // "FIP Rock", "France Inter"
    pub station_type: StationType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StationType {
    Main,                              // Station principale
    Webradio {                         // Webradio de n'importe quelle station
        parent_station: String,        // "fip", "francemusique", "mouv", etc.
    },
    LocalRadio { region: String },     // Radio locale France Bleu
}
```

### 2. Réponse API Live

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveResponse {
    pub station_name: String,
    pub delay_to_refresh: u64,        // millisecondes
    pub migrated: bool,
    pub now: ShowMetadata,
    pub next: Option<ShowMetadata>,
    pub local_radios: Option<Vec<LocalRadio>>,  // France Bleu uniquement
}
```

### 3. Métadonnées d'émission

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowMetadata {
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    pub producer: Option<String>,
    pub first_line: Line,          // Titre émission
    pub second_line: Line,         // Titre épisode/chronique
    pub third_line: Option<Line>,  // Sous-titre
    pub intro: Option<String>,     // Description
    pub song: Option<Song>,        // Pour radios musicales (FIP, France Musique)
    pub media: Media,              // Flux audio disponibles
    pub visual_background: Option<EmbedImage>,
    pub visuals: Option<Visuals>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Line {
    pub title: Option<String>,
    pub id: Option<String>,
    pub path: Option<String>,
}
```

### 4. Morceau musical (FIP, France Musique)

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Song {
    pub id: String,
    pub year: Option<u32>,
    pub interpreters: Vec<String>,
    pub release: Release,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub label: Option<String>,
    pub title: Option<String>,
    pub reference: Option<String>,
}
```

### 5. Flux audio

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Media {
    pub sources: Vec<StreamSource>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamSource {
    pub url: String,
    pub broadcast_type: BroadcastType,
    pub format: StreamFormat,
    pub bitrate: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BroadcastType {
    Live,
    Timeshift,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StreamFormat {
    Mp3,
    Aac,
    Hls,
}
```

### 6. Images

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedImage {
    pub model: String,
    pub src: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub dominant: Option<String>,
    pub copyright: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Visuals {
    pub card: Option<EmbedImage>,
    pub player: Option<EmbedImage>,
}

pub enum ImageSize {
    Tiny,      // 88x88
    Small,     // 200x200
    Medium,    // 420x720
    Large,     // 560x960
    XLarge,    // 1200x680
    Raw,       // Taille originale
}
```

### 7. Radios locales

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRadio {
    pub id: u32,
    pub title: String,
    pub name: String,
    pub is_on_air: bool,
}
```

---

## Méthodes principales

### 1. Création du client

```rust
impl RadioFranceClient {
    /// Créer un nouveau client avec settings par défaut
    pub async fn new() -> Result<Self> {
        Self::builder().build().await
    }

    /// Créer un builder pour configuration avancée
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Créer avec un reqwest::Client existant
    pub fn with_client(client: reqwest::Client) -> Self {
        Self {
            client,
            timeout: Duration::from_secs(30),
        }
    }
}
```

### 2. Découverte des stations

```rust
impl RadioFranceClient {
    /// Découvrir toutes les stations disponibles (scraping + API)
    pub async fn discover_all_stations(&self) -> Result<Vec<Station>> {
        let mut stations = Vec::new();

        // 1. Découvrir les stations principales
        let main_stations = self.scrape_main_stations().await?;

        // 2. Pour CHAQUE station principale, découvrir ses webradios éventuelles
        for main_station in main_stations {
            // Ajouter la station principale
            stations.push(main_station.clone());

            // Découvrir ses webradios (peut retourner 0 si aucune)
            if let Ok(webradios) = self.scrape_station_webradios(&main_station.slug).await {
                stations.extend(webradios);
            }
        }

        // 3. Cas spécial : radios locales France Bleu (via API)
        if let Ok(locals) = self.discover_local_radios().await {
            stations.extend(locals);
        }

        Ok(stations)
    }

    /// Scraper les stations principales depuis homepage
    async fn scrape_main_stations(&self) -> Result<Vec<Station>> {
        let html = self.client
            .get("https://www.radiofrance.fr/")
            .timeout(self.timeout)
            .send()
            .await?
            .text()
            .await?;

        let re = regex::Regex::new(
            r"(franceinter|franceinfo|franceculture|francemusique|fip|mouv|francebleu|monpetit)"
        )?;

        let mut slugs = std::collections::HashSet::new();
        for cap in re.captures_iter(&html) {
            slugs.insert(cap[0].to_string());
        }

        Ok(slugs.into_iter().map(|slug| Station {
            slug: slug.clone(),
            name: Self::slug_to_name(&slug),
            station_type: StationType::Main,
        }).collect())
    }

    /// Scraper les webradios d'une station donnée
    /// 
    /// Fonctionne pour n'importe quelle station (fip, francemusique, mouv, etc.)
    /// Retourne un Vec vide si aucune webradio n'est trouvée.
    async fn scrape_station_webradios(&self, station: &str) -> Result<Vec<Station>> {
        let url = format!("https://www.radiofrance.fr/{}", station);
        let html = self.client
            .get(&url)
            .timeout(self.timeout)
            .send()
            .await?
            .text()
            .await?;

        // Pattern générique : {station}_[a-z_]+
        let pattern = format!(r"{}_[a-z_]+", station);
        let re = regex::Regex::new(&pattern)?;

        let mut slugs = std::collections::HashSet::new();
        for cap in re.captures_iter(&html) {
            slugs.insert(cap[0].to_string());
        }

        Ok(slugs.into_iter().map(|slug| Station {
            slug: slug.clone(),
            name: Self::slug_to_name(&slug),
            station_type: StationType::Webradio {
                parent_station: station.to_string(),
            },
        }).collect())
    }

    /// Découvrir les radios locales France Bleu via API
    async fn discover_local_radios(&self) -> Result<Vec<Station>> {
        let response = self.live_metadata("francebleu").await?;
        
        Ok(response.local_radios
            .unwrap_or_default()
            .into_iter()
            .map(|local| Station {
                slug: local.name,
                name: local.title,
                station_type: StationType::LocalRadio {
                    region: local.title.replace("ICI ", ""),
                },
            })
            .collect())
    }

    /// Convertir slug en nom lisible (heuristique simple)
    fn slug_to_name(slug: &str) -> String {
        // Transformations basiques, à améliorer
        slug.replace('_', " ")
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}
```

### 3. Métadonnées live

```rust
impl RadioFranceClient {
    /// Récupérer les métadonnées live d'une station
    ///
    /// # Arguments
    /// * `station` - Slug de la station (ex: "franceculture", "fip_rock")
    ///
    /// # Webradios
    /// Pour les webradios FIP/France Musique, utiliser le format :
    /// - Principales : "fip", "francemusique"
    /// - Webradios : "fip_rock", "francemusique_jazz"
    ///
    /// L'API utilise le paramètre `?webradio=` automatiquement si nécessaire.
    pub async fn live_metadata(&self, station: &str) -> Result<LiveResponse> {
        let (base_station, webradio) = Self::parse_station_slug(station);
        
        let mut url = url::Url::parse(&format!(
            "https://www.radiofrance.fr/{}/api/live?",
            base_station
        ))?;

        // Ajouter le paramètre webradio si nécessaire
        if let Some(wr) = webradio {
            url.query_pairs_mut().append_pair("webradio", wr);
        }

        let response = self.client
            .get(url)
            .timeout(self.timeout)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Error::ApiError(format!(
                "API returned status: {}",
                response.status()
            )));
        }

        Ok(response.json().await?)
    }

    /// Parser le slug pour extraire station de base et webradio
    ///
    /// Exemples :
    /// - "fip" → ("fip", None)
    /// - "fip_rock" → ("fip", Some("fip_rock"))
    /// - "francemusique_jazz" → ("francemusique", Some("francemusique_jazz"))
    /// - "franceinter" → ("franceinter", None)
    fn parse_station_slug(slug: &str) -> (&str, Option<&str>) {
        if slug.starts_with("fip_") {
            ("fip", Some(slug))
        } else if slug.starts_with("francemusique_") {
            ("francemusique", Some(slug))
        } else if slug.starts_with("francebleu_") {
            // Radios locales : pas de paramètre webradio, slug direct
            (slug, None)
        } else {
            // Stations principales
            (slug, None)
        }
    }

    /// Récupérer uniquement les métadonnées de l'émission actuelle
    pub async fn now_playing(&self, station: &str) -> Result<ShowMetadata> {
        let response = self.live_metadata(station).await?;
        Ok(response.now)
    }
}
```

### 4. Flux audio (qualité maximale uniquement)

```rust
impl RadioFranceClient {
    /// Récupérer l'URL du flux audio en qualité maximale
    ///
    /// Priorité : AAC 192 kbps (hifi) > HLS
    pub async fn get_hifi_stream_url(&self, station: &str) -> Result<String> {
        let metadata = self.live_metadata(station).await?;
        
        // Chercher AAC hifi (192 kbps)
        if let Some(source) = metadata.now.media.sources.iter().find(|s| {
            s.format == StreamFormat::Aac 
            && s.broadcast_type == BroadcastType::Live
            && s.bitrate == 192
        }) {
            return Ok(source.url.clone());
        }

        // Fallback HLS
        if let Some(source) = metadata.now.media.sources.iter().find(|s| {
            s.format == StreamFormat::Hls
            && s.broadcast_type == BroadcastType::Live
        }) {
            return Ok(source.url.clone());
        }

        Err(Error::NoHifiStream(format!(
            "No HiFi stream found for station: {}",
            station
        )))
    }

    /// Lister tous les flux disponibles pour une station
    pub async fn get_available_streams(&self, station: &str) -> Result<Vec<StreamSource>> {
        let metadata = self.live_metadata(station).await?;
        Ok(metadata.now.media.sources)
    }
}
```

### 5. Images (Pikapi)

```rust
impl RadioFranceClient {
    /// Construire l'URL d'une image Pikapi
    ///
    /// # Arguments
    /// * `uuid` - UUID de l'image (extrait des métadonnées)
    /// * `size` - Taille souhaitée
    pub fn get_image_url(uuid: &str, size: ImageSize) -> String {
        let size_str = match size {
            ImageSize::Tiny => "88x88",
            ImageSize::Small => "200x200",
            ImageSize::Medium => "420x720",
            ImageSize::Large => "560x960",
            ImageSize::XLarge => "1200x680",
            ImageSize::Raw => "raw",
        };

        format!("https://www.radiofrance.fr/pikapi/images/{}/{}", uuid, size_str)
    }

    /// Extraire l'UUID d'une URL Pikapi existante
    pub fn extract_image_uuid(url: &str) -> Option<String> {
        let re = regex::Regex::new(r"/pikapi/images/([a-f0-9-]+)").ok()?;
        re.captures(url)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
    }
}
```

### 6. Polling intelligent

```rust
impl RadioFranceClient {
    /// Calculer le délai avant le prochain refresh recommandé
    pub fn next_refresh_delay(metadata: &LiveResponse) -> Duration {
        Duration::from_millis(metadata.delay_to_refresh)
    }

    /// Calculer le délai en tenant compte du temps écoulé
    pub fn adjusted_refresh_delay(
        metadata: &LiveResponse,
        fetched_at: std::time::SystemTime,
    ) -> Duration {
        let base_delay = Duration::from_millis(metadata.delay_to_refresh);
        let elapsed = fetched_at.elapsed().unwrap_or(Duration::ZERO);
        
        base_delay.saturating_sub(elapsed)
    }
}
```

---

## Exemple d'utilisation

### Découverte et affichage de toutes les stations

```rust
use pmoradiofrance::RadioFranceClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RadioFranceClient::new().await?;

    println!("Découverte des stations...");
    let stations = client.discover_all_stations().await?;

    println!("Trouvé {} stations :", stations.len());
    for station in &stations {
        println!("  - {} ({})", station.name, station.slug);
    }

    Ok(())
}
```

### Récupération des métadonnées live

```rust
use pmoradiofrance::RadioFranceClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RadioFranceClient::new().await?;

    // Station principale
    let fc_live = client.live_metadata("franceculture").await?;
    println!("France Culture : {} - {}",
        fc_live.now.first_line.title.unwrap_or_default(),
        fc_live.now.second_line.title.unwrap_or_default()
    );

    // Webradio FIP
    let fip_rock_live = client.live_metadata("fip_rock").await?;
    if let Some(song) = &fip_rock_live.now.song {
        println!("FIP Rock : {} - {}",
            song.interpreters.join(", "),
            fip_rock_live.now.first_line.title.unwrap_or_default()
        );
    }

    Ok(())
}
```

### Polling avec délai intelligent

```rust
use pmoradiofrance::RadioFranceClient;
use std::time::{Duration, SystemTime};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RadioFranceClient::new().await?;

    loop {
        let fetched_at = SystemTime::now();
        let metadata = client.live_metadata("fip").await?;

        println!("Now: {} - {}",
            metadata.now.second_line.title.unwrap_or_default(),
            metadata.now.first_line.title.unwrap_or_default()
        );

        // Attendre le délai recommandé
        let delay = RadioFranceClient::adjusted_refresh_delay(&metadata, fetched_at);
        tokio::time::sleep(delay).await;
    }
}
```

### Récupération du flux HiFi

```rust
use pmoradiofrance::RadioFranceClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RadioFranceClient::new().await?;

    let stream_url = client.get_hifi_stream_url("franceculture").await?;
    println!("Stream HiFi : {}", stream_url);
    // Exemple : https://icecast.radiofrance.fr/franceculture-hifi.aac?id=radiofrance

    Ok(())
}
```

---

## Points d'attention

### 1. Rate limiting

- Pas de limite documentée observée
- **Toujours** respecter `delayToRefresh` pour éviter les requêtes inutiles
- Mettre en cache les résultats de `discover_all_stations()` (TTL : 24h recommandé)

### 2. User-Agent

Pour un projet open-source, utiliser un User-Agent identifiable :

```rust
impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            user_agent: "PMOMusic/0.3.10 (https://github.com/votre-repo)".to_string(),
            // ...
        }
    }
}
```

### 3. Gestion d'erreurs

Les APIs peuvent retourner :
- **Données vides** (`null`) pour certains champs
- **`song`** absent pour radios non-musicales (France Inter, France Info, France Culture)
- **`localRadios`** uniquement pour France Bleu
- **`visual_background`** parfois absent

Toujours utiliser `Option<>` et gérer les cas manquants.

### 4. Webradios et paramètre `?webradio=`

- **Stations principales** : `/franceinter/api/live?`
- **Webradios FIP** : `/fip/api/live?webradio=fip_rock`
- **Webradios France Musique** : `/francemusique/api/live?webradio=francemusique_jazz`
- **Radios locales** : `/francebleu_alsace/api/live?` (slug direct, pas de paramètre)

### 5. Images Pikapi

Les URLs dans les réponses API utilisent parfois des chemins complets, parfois juste l'UUID :

```json
"src": "https://www.radiofrance.fr/pikapi/images/436430f7-5b2b-43f2-9f3c-28f2ad6cae39"
```

Toujours normaliser en extrayant l'UUID et en reconstruisant l'URL avec la taille souhaitée.

### 6. Scraping HTML

Le scraping HTML est **fragile** par nature. Recommandations :

- **Cache agressif** : Stocker les résultats de découverte (TTL 24h minimum)
- **Fallback** : Avoir une liste de base hardcodée si le scraping échoue
- **Validation optionnelle** : Tester chaque station découverte avec `/api/live?` avant de l'ajouter (peut être lent)
- **Monitoring** : Logger les échecs de découverte

### 7. Performance

Pour découvrir ~70 stations :
- **Scraping** : 1 homepage + 8 pages stations (une par station principale)
- **Validation France Bleu** : 1 requête API
- **Total** : ~10 requêtes HTTP

Temps estimé : 3-5 secondes avec timeout 30s (parallélisable pour réduire à ~1-2s).

### 8. Respect des CGU

- APIs publiques utilisées par le site officiel
- Usage acceptable pour un projet open-source personnel/non-commercial
- **Ne pas redistribuer** les flux audio commercialement
- **Ne pas surcharger** les serveurs (respecter `delayToRefresh`)

---

## Prochaines étapes

1. **Implémenter `client.rs`** avec l'architecture décrite
2. **Ajouter les tests** :
   - Tests unitaires pour parsing de slugs
   - Tests d'intégration pour découverte
   - Tests d'API live (avec captures VCR)
3. **Intégrer avec `pmosource`** :
   - Implémenter le trait `MusicSource`
   - Gérer le cache via `SourceCacheManager`
   - Support FIFO pour radios musicales (FIP)
4. **Documenter les limitations** :
   - Stations non accessibles
   - Cas d'erreur connus
   - Métriques de fiabilité

---

**Fin du rapport d'architecture client.rs**
