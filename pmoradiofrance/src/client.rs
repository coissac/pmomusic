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
use crate::models::{ImageSize, LiveResponse, ShowMetadata, Station, StreamSource};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::time::Duration;
use url::Url;

/// Default Radio France base URL
pub const DEFAULT_BASE_URL: &str = "https://www.radiofrance.fr";

/// Default timeout for HTTP requests (30 seconds)
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Default User-Agent
pub const DEFAULT_USER_AGENT: &str = "PMOMusic/0.3.10 (pmoradiofrance)";

/// Known main stations (fallback if scraping fails)
pub const KNOWN_MAIN_STATIONS: &[(&str, &str)] = &[
    ("franceinter", "France Inter"),
    ("franceinfo", "France Info"),
    ("franceculture", "France Culture"),
    ("francemusique", "France Musique"),
    ("fip", "FIP"),
    ("mouv", "Mouv'"),
    ("francebleu", "France Bleu"),
];

/// Radio France HTTP client
///
/// This client provides access to Radio France's public APIs for:
/// - Station discovery (main stations, webradios, local radios)
/// - Live metadata (current show, next show, stream URLs)
/// - Image URL construction (Pikapi)
///
/// The client is stateless and does not cache responses internally.
/// Caching should be handled by higher layers (e.g., config extension).
#[derive(Debug, Clone)]
pub struct RadioFranceClient {
    pub(crate) client: Client,
    base_url: String,
    timeout: Duration,
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

    /// Discover local France Bleu radios via API
    ///
    /// Uses the France Bleu /api/live? endpoint which includes
    /// a `localRadios` array in the `now` field.
    pub async fn discover_local_radios(&self) -> Result<Vec<Station>> {
        let response = self.live_metadata("francebleu").await?;

        Ok(response
            .local_radios()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|local| local.is_on_air)
            .map(|local| Station::new(local.name, local.title))
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
        let (base_station, webradio) = Self::parse_station_slug(station);

        let mut url = Url::parse(&format!("{}/{}/api/live", self.base_url, base_station))?;

        // Add webradio parameter if needed
        if let Some(wr) = webradio {
            url.query_pairs_mut().append_pair("webradio", wr);
        }

        #[cfg(feature = "logging")]
        tracing::debug!("Fetching live metadata: {}", url);

        let response = self.client.get(url).timeout(self.timeout).send().await?;

        if !response.status().is_success() {
            return Err(Error::ApiError(format!(
                "API returned status: {}",
                response.status()
            )));
        }

        let live: LiveResponse = response.json().await?;

        #[cfg(feature = "logging")]
        tracing::debug!(
            "Received metadata for {}: {} - {}",
            station,
            live.now.first_line.title_or_default(),
            live.now.second_line.title_or_default()
        );

        Ok(live)
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
        let metadata = self.live_metadata(station).await?;

        metadata
            .now
            .media
            .best_hifi_stream()
            .map(|s| s.url.clone())
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
            ("francebleu_alsace", None)
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
