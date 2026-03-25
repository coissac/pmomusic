//! HTTP client for Radio France API
//!
//! This module provides a client for accessing Radio France's public APIs,
//! including station discovery, live metadata, and stream URLs.
//!
//! # Example
//!
//! ```no_run
//! use pmoradiofrance::RadioFranceClient;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = RadioFranceClient::new().await?;
//!
//!     // Get live metadata for France Culture
//!     let live = client.live_metadata("franceculture").await?;
//!     println!("{} - {}",
//!         live.now.first_line.title_or_default(),
//!         live.now.second_line.title_or_default()
//!     );
//!
//!     // Get HiFi stream URL
//!     let stream_url = client.get_hifi_stream_url("franceculture").await?;
//!     println!("Stream: {}", stream_url);
//!
//!     Ok(())
//! }
//! ```

use crate::error::{Error, Result};
use crate::models::{
    EmbedImage, ImageSize, Line, LiveResponse, Media, PullResponse, ShowMetadata, Song, Station,
    StreamSource,
};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[cfg(feature = "pmoconfig")]
use crate::config_ext::StationInfo;

/// Default Radio France base URL
pub const DEFAULT_BASE_URL: &str = "https://www.radiofrance.fr";

/// Livemeta API base URL (new API since 2026)
pub const LIVEMETA_API_URL: &str = "https://api.radiofrance.fr/livemeta/pull";

/// Default timeout for HTTP requests (30 seconds)
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Default User-Agent
pub const DEFAULT_USER_AGENT: &str = "PMOMusic/0.3.10 (pmoradiofrance)";

// Données des stations — générées par tools/generate_radiofrance_stations.py
// Pour mettre à jour : python3 tools/generate_radiofrance_stations.py > pmoradiofrance/src/stations_data.rs
include!("stations_data.rs");

/// Radio France HTTP client
///
/// This client provides access to Radio France's public APIs for:
/// - Station discovery (main stations, webradios, local radios)
/// - Live metadata (current show, next show, stream URLs)
/// - Image URL construction (Pikapi)
///
/// The client holds an in-memory station mapping (slug → numeric ID + stream URL)
/// seeded from hardcoded constants. Use `with_station_mapping()` to inject a
/// mapping loaded from persistent storage (pmoconfig).
#[derive(Debug, Clone)]
pub struct RadioFranceClient {
    pub(crate) client: Client,
    base_url: String,
    timeout: Duration,
    /// Mapping slug → { station_id, stream_url } — thread-safe, updatable at runtime
    station_mapping: Arc<RwLock<HashMap<String, StationMappingEntry>>>,
}

/// Entry in the station mapping
#[derive(Debug, Clone)]
pub struct StationMappingEntry {
    pub station_id: u32,
    pub stream_url: String,
}

impl RadioFranceClient {
    /// Create a new client with default settings
    pub async fn new() -> Result<Self> {
        Self::builder().build().await
    }

    /// Create a builder for configuring the client
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Create a client with a custom reqwest::Client
    ///
    /// Useful for sharing HTTP connection pools or custom proxy settings
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout: Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
            station_mapping: Arc::new(RwLock::new(Self::default_station_mapping())),
        }
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the internal HTTP client
    pub fn http_client(&self) -> &Client {
        &self.client
    }

    /// Build the default station mapping from hardcoded constants
    fn default_station_mapping() -> HashMap<String, StationMappingEntry> {
        // Build a lookup for stream URLs
        let stream_map: HashMap<&str, &str> = STATION_STREAMS.iter().copied().collect();

        STATION_IDS
            .iter()
            .map(|(slug, id)| {
                let stream_url = stream_map
                    .get(slug)
                    .copied()
                    .unwrap_or_default()
                    .to_string();
                (
                    slug.to_string(),
                    StationMappingEntry {
                        station_id: *id,
                        stream_url,
                    },
                )
            })
            .collect()
    }

    /// Remplace le mapping en mémoire par un mapping chargé depuis le cache persistant
    ///
    /// Appelé au démarrage par MetadataCache pour charger le mapping pmoconfig.
    #[cfg(feature = "pmoconfig")]
    pub fn set_station_mapping(&self, mapping: HashMap<String, StationInfo>) {
        let mut m = self.station_mapping.write().unwrap();
        m.clear();
        for (slug, info) in mapping {
            m.insert(
                slug,
                StationMappingEntry {
                    station_id: info.station_id,
                    stream_url: info.stream_url,
                },
            );
        }
    }

    /// Récupère l'ensemble du mapping courant (pour persistance dans pmoconfig)
    #[cfg(feature = "pmoconfig")]
    pub fn get_station_mapping(&self) -> HashMap<String, StationInfo> {
        let m = self.station_mapping.read().unwrap();
        m.iter()
            .map(|(slug, entry)| {
                (
                    slug.clone(),
                    StationInfo {
                        station_id: entry.station_id,
                        stream_url: entry.stream_url.clone(),
                    },
                )
            })
            .collect()
    }

    /// Met à jour une entrée dans le mapping (utilisé après re-découverte unitaire)
    pub fn update_station_entry(&self, slug: &str, station_id: u32, stream_url: String) {
        let mut m = self.station_mapping.write().unwrap();
        m.insert(
            slug.to_string(),
            StationMappingEntry {
                station_id,
                stream_url,
            },
        );
    }

    /// Retourne l'ID numérique pour un slug, ou None si inconnu
    pub fn station_id_for_slug(&self, slug: &str) -> Option<u32> {
        let m = self.station_mapping.read().unwrap();
        m.get(slug).map(|e| e.station_id)
    }

    /// Retourne l'URL du stream HiFi pour un slug, ou None si inconnu
    pub fn stream_url_for_slug(&self, slug: &str) -> Option<String> {
        let m = self.station_mapping.read().unwrap();
        m.get(slug).map(|e| e.stream_url.clone())
    }

    // ========================================================================
    // Station Discovery
    // ========================================================================

    /// Discover all available stations
    ///
    /// This method discovers stations through multiple sources:
    /// 1. Main stations from the homepage
    /// 2. Webradios for each main station (FIP, France Musique, etc.)
    /// 3. Local radios from France Bleu API
    ///
    /// # Caching
    ///
    /// This method does NOT cache results. For caching, use the config extension
    /// which stores results with a configurable TTL.
    ///
    /// # Performance
    ///
    /// This method makes multiple HTTP requests (~10) and may take 3-5 seconds.
    /// Consider caching the results.
    pub async fn discover_all_stations(&self) -> Result<Vec<Station>> {
        let mut stations = Vec::new();

        // 1. Discover main stations
        let main_stations = self.discover_main_stations().await?;

        // 2. For each main station, discover webradios
        // Note: Skip francebleu because its "webradios" are actually local radios
        // which we get from the API with proper "ICI" names
        for main_station in &main_stations {
            // RÈGLE MÉTIER: Filtrer francebleu (portail générique, pas une vraie station)
            if main_station.slug == "francebleu" {
                continue;
            }

            stations.push(main_station.clone());

            // Try to discover webradios (may return empty for some stations)
            if let Ok(webradios) = self.discover_station_webradios(&main_station.slug).await {
                stations.extend(webradios);
            }
        }

        // 3. Discover France Bleu local radios via API (with correct "ICI" names)
        if let Ok(locals) = self.discover_local_radios().await {
            stations.extend(locals);
        }

        Ok(stations)
    }

    /// Discover main stations from the homepage
    ///
    /// Scrapes the Radio France homepage to find main station links.
    /// Falls back to known stations if scraping fails.
    pub async fn discover_main_stations(&self) -> Result<Vec<Station>> {
        let html = self
            .client
            .get(&self.base_url)
            .timeout(self.timeout)
            .send()
            .await?
            .text()
            .await?;

        let mut slugs = HashSet::new();

        // Method 1: Parse HTML and look for station links
        let document = Html::parse_document(&html);

        // Look for links to station pages
        if let Ok(selector) = Selector::parse("a[href]") {
            for element in document.select(&selector) {
                if let Some(href) = element.value().attr("href") {
                    // Match patterns like /franceculture, /fip, etc.
                    if let Some(slug) = self.extract_station_slug_from_href(href) {
                        slugs.insert(slug);
                    }
                }
            }
        }

        // Method 2: Regex fallback for station names in JavaScript/JSON
        let re = Regex::new(
            r#"["'/](franceinter|franceinfo|franceculture|francemusique|fip|mouv|francebleu)["'/]"#,
        )?;
        for cap in re.captures_iter(&html) {
            slugs.insert(cap[1].to_string());
        }

        // If we found stations, convert to Station objects
        if !slugs.is_empty() {
            return Ok(slugs
                .into_iter()
                .map(|slug| {
                    let name = Self::slug_to_display_name(&slug);
                    Station::new(slug, name)
                })
                .collect());
        }

        // Fallback to known stations
        #[cfg(feature = "logging")]
        tracing::warn!("Station discovery from HTML failed, using fallback list");

        Ok(KNOWN_MAIN_STATIONS
            .iter()
            .map(|(slug, name)| Station::new(*slug, *name))
            .collect())
    }

    /// Discover webradios for a given main station
    ///
    /// Scrapes the station page to find webradio identifiers.
    /// Works for FIP, France Musique, and potentially other stations.
    pub async fn discover_station_webradios(&self, station: &str) -> Result<Vec<Station>> {
        let url = format!("{}/{}", self.base_url, station);
        let html = self
            .client
            .get(&url)
            .timeout(self.timeout)
            .send()
            .await?
            .text()
            .await?;

        let mut slugs = HashSet::new();

        // Look for webradio identifiers in the HTML
        // Pattern: {station}_{variant} (e.g., fip_rock, francemusique_jazz)
        let pattern = format!(r#"["']({}_[a-z_]+)["']"#, regex::escape(station));
        let re = Regex::new(&pattern)?;

        for cap in re.captures_iter(&html) {
            let slug = cap[1].to_string();
            // Exclude the main station itself
            if slug != station {
                slugs.insert(slug);
            }
        }

        Ok(slugs
            .into_iter()
            .map(|slug| {
                let name = Self::slug_to_display_name(&slug);
                Station::new(slug, name)
            })
            .collect())
    }

    /// Discover local France Bleu radios
    ///
    /// Returns the list of France Bleu local radios from the static STATION_IDS mapping.
    ///
    /// The IDs in STATION_IDS were discovered via the livemeta/pull API in March 2026 and
    /// are stable (the API has been consistent since 2019). France Bleu's local radio
    /// network is essentially fixed, so a static list is sufficient and robust.
    pub async fn discover_local_radios(&self) -> Result<Vec<Station>> {
        Ok(STATION_IDS
            .iter()
            .filter(|(slug, _)| slug.starts_with("francebleu_"))
            .map(|(slug, _)| {
                let name = Self::slug_to_display_name(slug);
                Station::new(slug.to_string(), name)
            })
            .collect())
    }

    /// Extract station slug from a href attribute
    fn extract_station_slug_from_href(&self, href: &str) -> Option<String> {
        // Match patterns like:
        // - /franceculture
        // - /franceculture/...
        // - https://www.radiofrance.fr/fip
        let re = Regex::new(
            r"^(?:https?://[^/]+)?/(franceinter|franceinfo|franceculture|francemusique|fip|mouv|francebleu)(?:/|$)",
        )
        .ok()?;

        re.captures(href)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
    }

    /// Convert a station slug to a human-readable display name
    pub fn slug_to_display_name(slug: &str) -> String {
        // Handle webradio slugs (e.g., fip_rock -> FIP Rock)
        let parts: Vec<&str> = slug.split('_').collect();

        if parts.len() == 1 {
            // Main station
            match slug {
                "franceinter" => "France Inter".to_string(),
                "franceinfo" => "France Info".to_string(),
                "franceculture" => "France Culture".to_string(),
                "francemusique" => "France Musique".to_string(),
                "fip" => "FIP".to_string(),
                "mouv" => "Mouv'".to_string(),
                "francebleu" => "France Bleu".to_string(),
                _ => Self::capitalize_words(slug),
            }
        } else {
            // Webradio: combine parent name + variant
            let parent = match parts[0] {
                "fip" => "FIP",
                "francemusique" => "France Musique",
                "mouv" => "Mouv'",
                "francebleu" => "France Bleu",
                _ => return Self::capitalize_words(&slug.replace('_', " ")),
            };

            let variant = parts[1..]
                .iter()
                .map(|p| Self::capitalize_word(p))
                .collect::<Vec<_>>()
                .join(" ");

            format!("{} {}", parent, variant)
        }
    }

    fn capitalize_words(s: &str) -> String {
        s.split(|c: char| c == '_' || c == ' ')
            .map(Self::capitalize_word)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn capitalize_word(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().chain(chars).collect(),
        }
    }

    // ========================================================================
    // Live Metadata
    // ========================================================================

    /// Get live metadata for a station
    ///
    /// # Arguments
    ///
    /// * `station` - Station slug (e.g., "franceculture", "fip_rock", "francebleu_alsace")
    ///
    /// # Webradio Handling
    ///
    /// For webradios (e.g., "fip_rock"), the client automatically adds the
    /// `?webradio=` parameter to the API request.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pmoradiofrance::RadioFranceClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = RadioFranceClient::new().await?;
    ///
    /// // Main station
    /// let fc = client.live_metadata("franceculture").await?;
    ///
    /// // Webradio
    /// let fip_rock = client.live_metadata("fip_rock").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn live_metadata(&self, station: &str) -> Result<LiveResponse> {
        let station_id = self
            .station_id_for_slug(station)
            .ok_or_else(|| Error::ApiError(format!("Unknown station slug: {}", station)))?;

        let url = format!("{}/{}", LIVEMETA_API_URL, station_id);

        #[cfg(feature = "logging")]
        tracing::debug!("Fetching live metadata: {}", url);

        let response = self.client.get(&url).timeout(self.timeout).send().await?;

        if !response.status().is_success() {
            return Err(Error::ApiError(format!(
                "livemeta API returned status {} for station {} (id={})",
                response.status(),
                station,
                station_id
            )));
        }

        let pull: PullResponse = response.json().await?;

        // Fix 1: Vérifier que l'ID retourné correspond bien à la station demandée.
        // Si Radio France réassigne les IDs, on déclenche une redécouverte.
        if pull.station_id != station_id {
            #[cfg(feature = "logging")]
            tracing::warn!(
                "Station ID mismatch for {}: expected {}, API returned {} — triggering rediscovery",
                station, station_id, pull.station_id
            );
            // Mettre à jour le mapping avec le nouvel ID retourné par l'API
            let stream_url = self.stream_url_for_slug(station).unwrap_or_default();
            self.update_station_entry(station, pull.station_id, stream_url);
        }

        let live = self.pull_to_live_response(station, &pull);

        #[cfg(feature = "logging")]
        tracing::debug!(
            "Received metadata for {}: {} - {}",
            station,
            live.now.first_line.title_or_default(),
            live.now.second_line.title_or_default()
        );

        Ok(live)
    }

    /// Convertit une PullResponse (nouvelle API) en LiveResponse (format interne)
    fn pull_to_live_response(&self, station_slug: &str, pull: &PullResponse) -> LiveResponse {
        let now = if let Some(step) = pull.current_step() {
            self.step_to_show_metadata(station_slug, step)
        } else {
            ShowMetadata::default()
        };

        // delayToRefresh : on utilise end_time - now comme indicateur (min 30s)
        let delay_to_refresh = if let Some(end) = now.end_time {
            let current = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if end > current {
                ((end - current) * 1000).min(60_000) // max 60s, en ms
            } else {
                30_000
            }
        } else {
            30_000
        };

        LiveResponse {
            station_name: station_slug.to_string(),
            delay_to_refresh,
            migrated: true,
            now,
            next: None,
        }
    }

    /// Convertit un PullStep en ShowMetadata
    fn step_to_show_metadata(&self, station_slug: &str, step: &crate::models::PullStep) -> ShowMetadata {
        // Stream : injecté depuis le mapping en mémoire
        let stream_url = self.stream_url_for_slug(station_slug).unwrap_or_default();
        let sources = if !stream_url.is_empty() {
            vec![StreamSource {
                url: stream_url,
                broadcast_type: crate::models::BroadcastType::Live,
                format: crate::models::StreamFormat::Aac,
                bitrate: 192,
            }]
        } else {
            vec![]
        };

        // Visual : l'API peut retourner soit un UUID Pikapi, soit une URL S3/CDN complète
        let visual_background = step.visual.as_deref().map(|visual| {
            let src = if visual.starts_with("http") {
                // URL directe (S3, CDN) — utiliser telle quelle
                visual.to_string()
            } else {
                // UUID Pikapi — construire l'URL
                ImageSize::Large.build_url(visual)
            };
            EmbedImage {
                model: "EmbedImage".to_string(),
                src,
                width: None,
                height: None,
                dominant: None,
                copyright: None,
            }
        });

        if step.is_song() {
            // === Radio musicale ===
            let artists = step.artists_display();
            let song = Some(Song {
                id: step.song_id.clone().unwrap_or_default(),
                year: step.annee_edition_musique,
                interpreters: if artists.is_empty() {
                    vec![]
                } else {
                    vec![artists.clone()]
                },
                release: crate::models::Release {
                    label: step.label.clone(),
                    title: step.titre_album.clone(),
                    reference: None,
                },
            });

            ShowMetadata {
                start_time: step.start,
                end_time: step.end,
                producer: step.disc_jockey.clone(),
                first_line: Line {
                    title: Some(step.title.clone()),
                    id: None,
                    path: step.path.clone(),
                },
                second_line: Line {
                    title: if artists.is_empty() { None } else { Some(artists) },
                    id: None,
                    path: None,
                },
                song,
                media: Media { sources },
                visual_background,
                ..ShowMetadata::default()
            }
        } else {
            // === Radio parlée / émission ===
            let show_title = step.title_concept.as_deref()
                .unwrap_or(step.title.as_str())
                .to_string();
            let episode_title = if step.title_concept.is_some() {
                step.title.clone()
            } else {
                String::new()
            };
            let producer = step.disc_jockey.clone()
                .or_else(|| step.producers.first().map(|p| p.name.clone()));

            ShowMetadata {
                start_time: step.start,
                end_time: step.end,
                producer,
                first_line: Line {
                    title: Some(show_title),
                    id: None,
                    path: step.path.clone(),
                },
                second_line: Line {
                    title: if episode_title.is_empty() { None } else { Some(episode_title) },
                    id: None,
                    path: None,
                },
                intro: step.expression_description.clone()
                    .or_else(|| step.description.clone()),
                media: Media { sources },
                visual_background,
                ..ShowMetadata::default()
            }
        }
    }

    /// Tente de re-découvrir l'ID et l'URL stream d'une station depuis le web
    ///
    /// Utilisé quand un stream échoue ou qu'un slug est inconnu.
    /// Met à jour le mapping en mémoire si la découverte réussit.
    pub async fn rediscover_station(&self, slug: &str) -> Result<(u32, String)> {
        #[cfg(feature = "logging")]
        tracing::warn!("Rediscovering station mapping for: {}", slug);

        // Construire l'URL de la page de la station principale
        let base_slug = slug.split('_').next().unwrap_or(slug);
        let page_url = format!("{}/{}/__data.json", self.base_url, base_slug);

        let resp = self
            .client
            .get(&page_url)
            .timeout(self.timeout)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Error::ApiError(format!(
                "Cannot rediscover station {}: page returned {}",
                slug,
                resp.status()
            )));
        }

        let text = resp.text().await?;

        // Fix 2: chercher l'ID spécifique à la station via son brandEnum.
        // Ex: "fip_rock" → "FIP_ROCK" → cherche `"FIP_ROCK","64"` dans le JSON SvelteKit.
        // Pour les stations principales, on cherche aussi le pattern `"Brand","<id>"`.
        let brand_enum = slug.to_uppercase(); // fip_rock → FIP_ROCK
        let re_enum = Regex::new(&format!(r#""{}","(\d+)""#, regex::escape(&brand_enum)))?;
        let brand_id: Option<u32> = re_enum
            .captures_iter(&text)
            .filter_map(|c| c[1].parse::<u32>().ok())
            .next();

        // Fallback : premier "Brand","<id>" dans la page (pour stations principales)
        let brand_id = if brand_id.is_some() {
            brand_id
        } else {
            let re_brand = Regex::new(r#""Brand","(\d+)""#)?;
            let ids: Vec<u32> = re_brand
                .captures_iter(&text)
                .filter_map(|c| c[1].parse::<u32>().ok())
                .collect();
            ids.into_iter().next()
        };

        let station_id = brand_id.ok_or_else(|| {
            Error::ApiError(format!("Could not find station_id for {} in page data", slug))
        })?;

        // Dériver l'URL stream depuis le slug (format icecast connu)
        let stream_url = Self::derive_stream_url(slug);

        self.update_station_entry(slug, station_id, stream_url.clone());

        #[cfg(feature = "logging")]
        tracing::info!(
            "Rediscovered station {}: id={}, stream={}",
            slug,
            station_id,
            stream_url
        );

        Ok((station_id, stream_url))
    }

    /// Valide qu'une station existe toujours et que son stream est accessible
    ///
    /// Vérifie deux choses :
    /// 1. L'API livemeta répond avec un stationId valide (station connue de Radio France)
    /// 2. L'URL icecast répond HTTP 200 (stream physiquement disponible)
    ///
    /// Retourne Ok(()) si tout est OK, Err si la station est invalide ou le stream mort.
    pub async fn validate_station(&self, slug: &str) -> Result<()> {
        // 1. Vérifier que l'API livemeta répond pour cette station
        let station_id = self
            .station_id_for_slug(slug)
            .ok_or_else(|| Error::ApiError(format!("Unknown station slug: {}", slug)))?;

        let url = format!("{}/{}", LIVEMETA_API_URL, station_id);
        let response = self.client.get(&url).timeout(self.timeout).send().await?;

        if !response.status().is_success() {
            return Err(Error::ApiError(format!(
                "Station {} (id={}) no longer exists: livemeta returned {}",
                slug, station_id, response.status()
            )));
        }

        let pull: PullResponse = response.json().await?;

        // Vérifier cohérence de l'ID — si mismatch, la station a peut-être été réassignée
        if pull.station_id != station_id {
            return Err(Error::ApiError(format!(
                "Station {} ID mismatch: stored={}, API returned={} — mapping is stale",
                slug, station_id, pull.station_id
            )));
        }

        // 2. Vérifier que l'URL icecast répond
        if let Some(stream_url) = self.stream_url_for_slug(slug) {
            if !stream_url.is_empty() {
                let stream_resp = self
                    .client
                    .head(&stream_url)
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await?;
                if !stream_resp.status().is_success() {
                    return Err(Error::ApiError(format!(
                        "Stream for {} is not accessible: {} returned {}",
                        slug, stream_url, stream_resp.status()
                    )));
                }
            }
        }

        Ok(())
    }

    /// Dérive l'URL icecast depuis le slug selon le pattern Radio France
    pub fn derive_stream_url(slug: &str) -> String {
        // fip_rock → fiprock, fip_sacre_francais → fipsacrefrancais
        let name = slug.replace('_', "");
        format!("https://icecast.radiofrance.fr/{}-hifi.aac", name)
    }

    /// Get only the current show metadata
    pub async fn now_playing(&self, station: &str) -> Result<ShowMetadata> {
        let response = self.live_metadata(station).await?;
        Ok(response.now)
    }

    /// Parse a station slug to extract base station and optional webradio
    ///
    /// # Examples
    ///
    /// - "fip" → ("fip", None)
    /// - "fip_rock" → ("fip", Some("fip_rock"))
    /// - "francemusique_jazz" → ("francemusique", Some("francemusique_jazz"))
    /// - "francebleu_alsace" → ("francebleu_alsace", None) - local radios don't use webradio param
    pub fn parse_station_slug(slug: &str) -> (&str, Option<&str>) {
        // FIP webradios
        if slug.starts_with("fip_") {
            return ("fip", Some(slug));
        }

        // France Musique webradios
        if slug.starts_with("francemusique_") {
            return ("francemusique", Some(slug));
        }

        // Mouv' webradios (if any)
        if slug.starts_with("mouv_") {
            return ("mouv", Some(slug));
        }

        // France Bleu local radios are webradios of francebleu
        // e.g., francebleu_alsace → francebleu/api/live?webradio=francebleu-alsace
        if slug.starts_with("francebleu_") {
            return ("francebleu", Some(slug));
        }

        // Main stations
        (slug, None)
    }

    // ========================================================================
    // Stream URLs
    // ========================================================================

    /// Get the best HiFi stream URL for a station
    ///
    /// Prioritizes AAC 192 kbps, falls back to HLS.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pmoradiofrance::RadioFranceClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = RadioFranceClient::new().await?;
    /// let url = client.get_hifi_stream_url("franceculture").await?;
    /// // Returns: https://icecast.radiofrance.fr/franceculture-hifi.aac?id=radiofrance
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_hifi_stream_url(&self, station: &str) -> Result<String> {
        self.stream_url_for_slug(station)
            .ok_or_else(|| Error::NoHifiStream(station.to_string()))
    }

    /// Get all available stream sources for a station
    pub async fn get_available_streams(&self, station: &str) -> Result<Vec<StreamSource>> {
        let metadata = self.live_metadata(station).await?;
        Ok(metadata.now.media.sources)
    }

    // ========================================================================
    // Image URLs (Pikapi)
    // ========================================================================

    /// Build a Pikapi image URL from a UUID
    ///
    /// # Arguments
    ///
    /// * `uuid` - Image UUID (e.g., "436430f7-5b2b-43f2-9f3c-28f2ad6cae39")
    /// * `size` - Desired image size
    pub fn build_image_url(uuid: &str, size: ImageSize) -> String {
        size.build_url(uuid)
    }

    /// Extract UUID from a Pikapi URL
    ///
    /// # Example
    ///
    /// ```
    /// use pmoradiofrance::RadioFranceClient;
    ///
    /// let url = "https://www.radiofrance.fr/pikapi/images/436430f7-5b2b-43f2-9f3c-28f2ad6cae39/200x200";
    /// let uuid = RadioFranceClient::extract_image_uuid(url);
    /// assert_eq!(uuid, Some("436430f7-5b2b-43f2-9f3c-28f2ad6cae39".to_string()));
    /// ```
    pub fn extract_image_uuid(url: &str) -> Option<String> {
        let re = Regex::new(r"/pikapi/images/([a-f0-9-]+)").ok()?;
        re.captures(url)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
    }

    // ========================================================================
    // Polling Helpers
    // ========================================================================

    /// Get the recommended delay before the next metadata refresh
    ///
    /// Uses the `delayToRefresh` field from the API response.
    pub fn next_refresh_delay(metadata: &LiveResponse) -> Duration {
        Duration::from_millis(metadata.delay_to_refresh)
    }

    /// Calculate the adjusted refresh delay accounting for elapsed time
    ///
    /// # Arguments
    ///
    /// * `metadata` - The metadata response
    /// * `fetched_at` - When the metadata was fetched
    pub fn adjusted_refresh_delay(
        metadata: &LiveResponse,
        fetched_at: std::time::SystemTime,
    ) -> Duration {
        let base_delay = Duration::from_millis(metadata.delay_to_refresh);
        let elapsed = fetched_at.elapsed().unwrap_or(Duration::ZERO);
        base_delay.saturating_sub(elapsed)
    }
}

/// Builder for configuring a RadioFranceClient
#[derive(Debug)]
pub struct ClientBuilder {
    client: Option<Client>,
    base_url: String,
    timeout: Duration,
    user_agent: String,
    proxy: Option<String>,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            client: None,
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout: Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
            user_agent: DEFAULT_USER_AGENT.to_string(),
            proxy: None,
        }
    }
}

impl ClientBuilder {
    /// Create a new builder with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a custom HTTP client
    pub fn client(mut self, client: Client) -> Self {
        self.client = Some(client);
        self
    }

    /// Set the base URL
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the request timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set a custom User-Agent header
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Set a proxy URL
    pub fn proxy(mut self, proxy: impl Into<String>) -> Self {
        self.proxy = Some(proxy.into());
        self
    }

    /// Build the client
    pub async fn build(self) -> Result<RadioFranceClient> {
        let client = if let Some(client) = self.client {
            client
        } else {
            let mut builder = Client::builder()
                .user_agent(&self.user_agent)
                .timeout(self.timeout);

            if let Some(proxy_url) = &self.proxy {
                let proxy = reqwest::Proxy::all(proxy_url)
                    .map_err(|e| Error::other(format!("Invalid proxy: {}", e)))?;
                builder = builder.proxy(proxy);
            }

            builder.build()?
        };

        Ok(RadioFranceClient {
            client,
            base_url: self.base_url,
            timeout: self.timeout,
            station_mapping: Arc::new(RwLock::new(RadioFranceClient::default_station_mapping())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Unit Tests (no network)
    // ========================================================================

    #[test]
    fn test_parse_station_slug() {
        assert_eq!(RadioFranceClient::parse_station_slug("fip"), ("fip", None));
        assert_eq!(
            RadioFranceClient::parse_station_slug("fip_rock"),
            ("fip", Some("fip_rock"))
        );
        assert_eq!(
            RadioFranceClient::parse_station_slug("francemusique_jazz"),
            ("francemusique", Some("francemusique_jazz"))
        );
        assert_eq!(
            RadioFranceClient::parse_station_slug("francebleu_alsace"),
            ("francebleu", Some("francebleu_alsace"))
        );
        assert_eq!(
            RadioFranceClient::parse_station_slug("franceculture"),
            ("franceculture", None)
        );
    }

    #[test]
    fn test_slug_to_display_name() {
        assert_eq!(
            RadioFranceClient::slug_to_display_name("franceculture"),
            "France Culture"
        );
        assert_eq!(RadioFranceClient::slug_to_display_name("fip"), "FIP");
        assert_eq!(
            RadioFranceClient::slug_to_display_name("fip_rock"),
            "FIP Rock"
        );
        assert_eq!(
            RadioFranceClient::slug_to_display_name("francemusique_la_jazz"),
            "France Musique La Jazz"
        );
    }

    #[test]
    fn test_extract_image_uuid() {
        let url =
            "https://www.radiofrance.fr/pikapi/images/436430f7-5b2b-43f2-9f3c-28f2ad6cae39/200x200";
        assert_eq!(
            RadioFranceClient::extract_image_uuid(url),
            Some("436430f7-5b2b-43f2-9f3c-28f2ad6cae39".to_string())
        );

        let url_no_size =
            "https://www.radiofrance.fr/pikapi/images/436430f7-5b2b-43f2-9f3c-28f2ad6cae39";
        assert_eq!(
            RadioFranceClient::extract_image_uuid(url_no_size),
            Some("436430f7-5b2b-43f2-9f3c-28f2ad6cae39".to_string())
        );
    }

    #[test]
    fn test_builder_defaults() {
        let builder = ClientBuilder::default();
        assert_eq!(builder.base_url, DEFAULT_BASE_URL);
        assert_eq!(
            builder.timeout,
            Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS)
        );
    }

    // ========================================================================
    // Integration Tests (real API calls)
    //
    // Run with: cargo test -p pmoradiofrance -- --ignored
    // ========================================================================

    /// Test client creation
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_client_creation() {
        let client = RadioFranceClient::new().await;
        assert!(
            client.is_ok(),
            "Failed to create client: {:?}",
            client.err()
        );
    }

    /// Test live metadata for France Culture (talk radio)
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_live_metadata_franceculture() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let metadata = client.live_metadata("franceculture").await;

        assert!(
            metadata.is_ok(),
            "Failed to get France Culture metadata: {:?}",
            metadata.err()
        );

        let metadata = metadata.unwrap();
        assert_eq!(metadata.station_name, "franceculture");
        assert!(
            metadata.delay_to_refresh > 0,
            "delay_to_refresh should be positive"
        );

        // France Culture should have show info
        assert!(
            metadata.now.first_line.title.is_some() || metadata.now.second_line.title.is_some(),
            "Expected at least one title line"
        );

        // Should have media sources
        assert!(
            !metadata.now.media.sources.is_empty(),
            "Expected media sources"
        );

        println!(
            "France Culture - Now: {} - {}",
            metadata.now.first_line.title_or_default(),
            metadata.now.second_line.title_or_default()
        );
        println!("  Producer: {:?}", metadata.now.producer);
        println!("  Delay to refresh: {} ms", metadata.delay_to_refresh);
        println!("  Sources: {} available", metadata.now.media.sources.len());
    }

    /// Test live metadata for France Inter
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_live_metadata_franceinter() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let metadata = client.live_metadata("franceinter").await;

        assert!(
            metadata.is_ok(),
            "Failed to get France Inter metadata: {:?}",
            metadata.err()
        );

        let metadata = metadata.unwrap();
        assert_eq!(metadata.station_name, "franceinter");
        assert!(
            !metadata.now.media.sources.is_empty(),
            "Expected media sources"
        );

        println!(
            "France Inter - Now: {} - {}",
            metadata.now.first_line.title_or_default(),
            metadata.now.second_line.title_or_default()
        );
    }

    /// Test live metadata for FIP (music radio - should have song info)
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_live_metadata_fip() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let metadata = client.live_metadata("fip").await;

        assert!(
            metadata.is_ok(),
            "Failed to get FIP metadata: {:?}",
            metadata.err()
        );

        let metadata = metadata.unwrap();
        assert_eq!(metadata.station_name, "fip");

        // FIP often has song info (but not always during talk segments)
        if let Some(song) = &metadata.now.song {
            println!(
                "FIP - Now playing: {} - {}",
                song.artists_display(),
                metadata.now.first_line.title_or_default()
            );
            if let Some(album) = &song.release.title {
                println!("  Album: {}", album);
            }
        } else {
            println!(
                "FIP - Now: {} (no song info)",
                metadata.now.first_line.title_or_default()
            );
        }
    }

    /// Test live metadata for FIP Rock webradio
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_live_metadata_fip_rock() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let metadata = client.live_metadata("fip_rock").await;

        assert!(
            metadata.is_ok(),
            "Failed to get FIP Rock metadata: {:?}",
            metadata.err()
        );

        let metadata = metadata.unwrap();
        // API returns "fip" as station_name even for webradios
        assert!(
            metadata.station_name == "fip" || metadata.station_name == "fip_rock",
            "Unexpected station name: {}",
            metadata.station_name
        );

        println!(
            "FIP Rock - Now: {}",
            metadata.now.first_line.title_or_default()
        );
    }

    /// Test live metadata for France Musique
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_live_metadata_francemusique() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let metadata = client.live_metadata("francemusique").await;

        assert!(
            metadata.is_ok(),
            "Failed to get France Musique metadata: {:?}",
            metadata.err()
        );

        let metadata = metadata.unwrap();
        assert_eq!(metadata.station_name, "francemusique");

        println!(
            "France Musique - Now: {} - {}",
            metadata.now.first_line.title_or_default(),
            metadata.now.second_line.title_or_default()
        );
    }

    /// Test live metadata for France Bleu (should have local radios)
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_live_metadata_francebleu() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let metadata = client.live_metadata("francebleu").await;

        assert!(
            metadata.is_ok(),
            "Failed to get France Bleu metadata: {:?}",
            metadata.err()
        );

        let metadata = metadata.unwrap();
        assert_eq!(metadata.station_name, "francebleu");

        // France Bleu should have local radios (in now.local_radios)
        if let Some(locals) = metadata.local_radios() {
            println!("France Bleu - {} local radios found", locals.len());
            assert!(!locals.is_empty(), "Expected local radios for France Bleu");

            // Print first 5 local radios
            for local in locals.iter().take(5) {
                println!("  - {} ({})", local.title, local.name);
            }
        } else {
            panic!("Expected local_radios field for France Bleu");
        }
    }

    /// Test live metadata for Mouv'
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_live_metadata_mouv() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let metadata = client.live_metadata("mouv").await;

        assert!(
            metadata.is_ok(),
            "Failed to get Mouv' metadata: {:?}",
            metadata.err()
        );

        let metadata = metadata.unwrap();
        assert_eq!(metadata.station_name, "mouv");

        println!(
            "Mouv' - Now: {} - {}",
            metadata.now.first_line.title_or_default(),
            metadata.now.second_line.title_or_default()
        );
    }

    /// Test HiFi stream URL retrieval
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_get_hifi_stream_url() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");

        // Test multiple stations
        for station in &["franceculture", "franceinter", "fip", "francemusique"] {
            let url = client.get_hifi_stream_url(station).await;
            assert!(
                url.is_ok(),
                "Failed to get HiFi stream for {}: {:?}",
                station,
                url.err()
            );

            let url = url.unwrap();
            assert!(
                url.contains("icecast.radiofrance.fr") || url.contains("stream.radiofrance.fr"),
                "Unexpected stream URL for {}: {}",
                station,
                url
            );

            println!("{}: {}", station, url);
        }
    }

    /// Test available streams listing
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_get_available_streams() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let streams = client.get_available_streams("franceculture").await;

        assert!(
            streams.is_ok(),
            "Failed to get streams: {:?}",
            streams.err()
        );

        let streams = streams.unwrap();
        assert!(!streams.is_empty(), "Expected at least one stream");

        println!("France Culture streams:");
        for stream in &streams {
            println!(
                "  - {} {:?} {} kbps: {}",
                stream.format.mime_type(),
                stream.broadcast_type,
                stream.bitrate,
                stream.url
            );
        }

        // Should have at least AAC and HLS
        let has_aac = streams
            .iter()
            .any(|s| s.format == crate::models::StreamFormat::Aac);
        let has_hls = streams
            .iter()
            .any(|s| s.format == crate::models::StreamFormat::Hls);

        assert!(has_aac || has_hls, "Expected at least AAC or HLS stream");
    }

    /// Test main station discovery
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_discover_main_stations() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let stations = client.discover_main_stations().await;

        assert!(
            stations.is_ok(),
            "Failed to discover main stations: {:?}",
            stations.err()
        );

        let stations = stations.unwrap();
        assert!(!stations.is_empty(), "Expected at least one main station");

        println!("Discovered {} main stations:", stations.len());
        for station in &stations {
            println!("  - {} ({})", station.name, station.slug);
        }

        // Should have the core stations
        let slugs: Vec<&str> = stations.iter().map(|s| s.slug.as_str()).collect();
        assert!(
            slugs.contains(&"franceinter")
                || slugs.contains(&"franceculture")
                || slugs.contains(&"fip"),
            "Expected at least one of franceinter, franceculture, or fip"
        );
    }

    /// Test FIP webradios discovery
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_discover_fip_webradios() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let webradios = client.discover_station_webradios("fip").await;

        assert!(
            webradios.is_ok(),
            "Failed to discover FIP webradios: {:?}",
            webradios.err()
        );

        let webradios = webradios.unwrap();
        println!("Discovered {} FIP webradios:", webradios.len());
        for wr in &webradios {
            println!("  - {} ({})", wr.name, wr.slug);
        }

        // FIP should have multiple webradios (rock, jazz, etc.)
        if !webradios.is_empty() {
            // At least verify they have the right parent
            for wr in &webradios {
                assert!(
                    wr.slug.starts_with("fip_"),
                    "Webradio slug should start with fip_: {}",
                    wr.slug
                );
            }
        }
    }

    /// Test France Musique webradios discovery
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_discover_francemusique_webradios() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let webradios = client.discover_station_webradios("francemusique").await;

        assert!(
            webradios.is_ok(),
            "Failed to discover France Musique webradios: {:?}",
            webradios.err()
        );

        let webradios = webradios.unwrap();
        println!("Discovered {} France Musique webradios:", webradios.len());
        for wr in &webradios {
            println!("  - {} ({})", wr.name, wr.slug);
        }
    }

    /// Test local radios discovery (France Bleu)
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_discover_local_radios() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let locals = client.discover_local_radios().await;

        assert!(
            locals.is_ok(),
            "Failed to discover local radios: {:?}",
            locals.err()
        );

        let locals = locals.unwrap();
        assert!(!locals.is_empty(), "Expected local radios from France Bleu");

        println!("Discovered {} France Bleu local radios:", locals.len());
        for local in locals.iter().take(10) {
            println!("  - {} ({})", local.name, local.slug);
        }

        // Should have ~40 local radios
        assert!(
            locals.len() >= 30,
            "Expected at least 30 local radios, got {}",
            locals.len()
        );
    }

    /// Test full station discovery
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_discover_all_stations() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let stations = client.discover_all_stations().await;

        assert!(
            stations.is_ok(),
            "Failed to discover all stations: {:?}",
            stations.err()
        );

        let stations = stations.unwrap();
        assert!(!stations.is_empty(), "Expected stations");

        println!("Discovered {} total stations", stations.len());

        // Should have a significant number of stations
        assert!(
            stations.len() >= 40,
            "Expected at least 40 total stations, got {}",
            stations.len()
        );
    }

    /// Test that invalid station returns an error
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_invalid_station() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let result = client.live_metadata("nonexistent_station_xyz").await;

        // Should fail with API error or similar
        assert!(result.is_err(), "Expected error for invalid station");
        println!("Got expected error: {:?}", result.err());
    }

    /// Test refresh delay calculation
    #[tokio::test]
    #[ignore = "Integration test - calls real Radio France API"]
    async fn test_refresh_delay() {
        let client = RadioFranceClient::new()
            .await
            .expect("Failed to create client");
        let metadata = client
            .live_metadata("franceculture")
            .await
            .expect("Failed to get metadata");

        let delay = RadioFranceClient::next_refresh_delay(&metadata);
        assert!(delay.as_millis() > 0, "Expected positive delay");

        println!("Recommended refresh delay: {:?}", delay);

        // Test adjusted delay
        let fetched_at = std::time::SystemTime::now();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let adjusted = RadioFranceClient::adjusted_refresh_delay(&metadata, fetched_at);

        assert!(
            adjusted < delay,
            "Adjusted delay should be less than original"
        );
        println!("Adjusted delay after 100ms: {:?}", adjusted);
    }
}
