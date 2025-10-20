//! HTTP client for Radio Paradise API

use crate::error::{Error, Result};
use crate::models::{Bitrate, Block, EventId, NowPlaying};
use reqwest::Client;
use std::time::Duration;
use url::Url;

/// Default Radio Paradise API base URL
pub const DEFAULT_API_BASE: &str = "https://api.radioparadise.com/api";

/// Default block base URL pattern
pub const DEFAULT_BLOCK_BASE: &str = "https://apps.radioparadise.com/blocks/chan/0";

/// Default image base URL
pub const DEFAULT_IMAGE_BASE: &str = "https://img.radioparadise.com/covers/l/";

/// Default timeout for HTTP requests
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default User-Agent
pub const DEFAULT_USER_AGENT: &str = "pmoparadise/0.1.0";

/// Radio Paradise HTTP client
///
/// This client provides access to Radio Paradise's streaming API,
/// including metadata retrieval and block streaming.
///
/// # Example
///
/// ```no_run
/// use pmoparadise::RadioParadiseClient;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let client = RadioParadiseClient::new().await?;
///     let now_playing = client.now_playing().await?;
///     println!("Now playing: {} - {}",
///              now_playing.current_song.as_ref().unwrap().artist,
///              now_playing.current_song.as_ref().unwrap().title);
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct RadioParadiseClient {
    pub(crate) client: Client,
    api_base: String,
    block_base: String,
    image_base: String,
    bitrate: Bitrate,
    channel: u8,
    pub(crate) timeout: Duration,
    next_block_url: Option<String>,
}

impl RadioParadiseClient {
    /// Create a new client with default settings
    ///
    /// Uses FLAC quality (bitrate 4) and channel 0 (main mix)
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
            api_base: DEFAULT_API_BASE.to_string(),
            block_base: DEFAULT_BLOCK_BASE.to_string(),
            image_base: DEFAULT_IMAGE_BASE.to_string(),
            bitrate: Bitrate::default(),
            channel: 0,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            next_block_url: None,
        }
    }

    /// Get the current bitrate setting
    pub fn bitrate(&self) -> Bitrate {
        self.bitrate
    }

    /// Get the current channel (0 = main mix)
    pub fn channel(&self) -> u8 {
        self.channel
    }

    fn block_base_for_channel(channel: u8) -> String {
        format!("https://apps.radioparadise.com/blocks/chan/{}", channel)
    }

    /// Clone the client with a different channel while preserving other settings.
    pub fn clone_with_channel(&self, channel: u8) -> Self {
        let mut cloned = self.clone();
        cloned.channel = channel;
        cloned.block_base = Self::block_base_for_channel(channel);
        cloned.next_block_url = None;
        cloned
    }

    /// Clone the client with a different bitrate while preserving other settings.
    pub fn clone_with_bitrate(&self, bitrate: Bitrate) -> Self {
        let mut cloned = self.clone();
        cloned.bitrate = bitrate;
        cloned.next_block_url = None;
        cloned
    }

    /// Clone the client with an updated channel and bitrate.
    pub fn clone_with_channel_and_bitrate(&self, channel: u8, bitrate: Bitrate) -> Self {
        let mut cloned = self.clone_with_channel(channel);
        cloned.bitrate = bitrate;
        cloned
    }

    /// Get a block by event ID
    ///
    /// If `event` is None, returns the current block.
    ///
    /// # Arguments
    ///
    /// * `event` - Optional event ID to fetch a specific block
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pmoparadise::RadioParadiseClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = RadioParadiseClient::new().await?;
    ///
    /// // Get current block
    /// let current = client.get_block(None).await?;
    /// println!("Current block: {} songs", current.song_count());
    ///
    /// // Get next block
    /// let next = client.get_block(Some(current.end_event)).await?;
    /// println!("Next block: {} songs", next.song_count());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_block(&self, event: Option<EventId>) -> Result<Block> {
        let mut url = Url::parse(&format!("{}/get_block", self.api_base))?;

        url.query_pairs_mut()
            .append_pair("bitrate", &self.bitrate.as_u8().to_string())
            .append_pair("info", "true")
            .append_pair("channel", &self.channel.to_string());

        if let Some(event_id) = event {
            url.query_pairs_mut()
                .append_pair("event", &event_id.to_string());
        }

        #[cfg(feature = "logging")]
        tracing::debug!("Fetching block: {}", url);

        let response = self.client.get(url).timeout(self.timeout).send().await?;

        if !response.status().is_success() {
            return Err(Error::other(format!(
                "API returned error status: {}",
                response.status()
            )));
        }

        let mut block: Block = response.json().await?;

        // Set image_base if not provided
        if block.image_base.is_none() {
            block.image_base = Some(self.image_base.clone());
        }

        #[cfg(feature = "logging")]
        tracing::debug!(
            "Received block: event={}, songs={}",
            block.event,
            block.song_count()
        );

        Ok(block)
    }

    /// Get the currently playing block and song
    ///
    /// Returns a `NowPlaying` struct with the current block and
    /// an estimate of which song is currently playing (first song).
    ///
    /// Note: Without real-time synchronization, we assume playback
    /// starts from the beginning of the block.
    pub async fn now_playing(&self) -> Result<NowPlaying> {
        let block = self.get_block(None).await?;
        Ok(NowPlaying::from_block(block))
    }

    /// Get the full URL for a cover image
    ///
    /// # Arguments
    ///
    /// * `cover_path` - The cover filename/path from song metadata
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pmoparadise::RadioParadiseClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = RadioParadiseClient::new().await?;
    /// let url = client.cover_url("B00000I0JF.jpg")?;
    /// println!("Cover URL: {}", url);
    /// # Ok(())
    /// # }
    /// ```
    pub fn cover_url(&self, cover_path: &str) -> Result<Url> {
        let url_str = format!("{}{}", self.image_base, cover_path);
        Ok(Url::parse(&url_str)?)
    }

    /// Prefetch metadata for the next block
    ///
    /// Stores the next block URL internally for seamless transitions.
    /// Call this before the current block finishes playing.
    ///
    /// # Arguments
    ///
    /// * `current` - The currently playing block
    pub async fn prefetch_next(&mut self, current: &Block) -> Result<()> {
        let next_block = self.get_block(Some(current.end_event)).await?;
        self.next_block_url = Some(next_block.url.clone());

        #[cfg(feature = "logging")]
        tracing::debug!(
            "Prefetched next block: {} -> {}",
            current.end_event,
            next_block.event
        );

        Ok(())
    }

    /// Get the prefetched next block URL
    pub fn next_block_url(&self) -> Option<&str> {
        self.next_block_url.as_deref()
    }

    /// Clear the prefetched next block URL
    pub fn clear_next_block(&mut self) {
        self.next_block_url = None;
    }

    /// Get the internal HTTP client
    pub fn http_client(&self) -> &Client {
        &self.client
    }
}

/// Builder for configuring a RadioParadiseClient
#[derive(Debug)]
pub struct ClientBuilder {
    client: Option<Client>,
    api_base: String,
    block_base: String,
    image_base: String,
    bitrate: Bitrate,
    channel: u8,
    timeout: Duration,
    user_agent: String,
    proxy: Option<String>,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            client: None,
            api_base: DEFAULT_API_BASE.to_string(),
            block_base: DEFAULT_BLOCK_BASE.to_string(),
            image_base: DEFAULT_IMAGE_BASE.to_string(),
            bitrate: Bitrate::default(),
            channel: 0,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
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

    /// Set the API base URL
    pub fn api_base(mut self, url: impl Into<String>) -> Self {
        self.api_base = url.into();
        self
    }

    /// Set the block base URL
    pub fn block_base(mut self, url: impl Into<String>) -> Self {
        self.block_base = url.into();
        self
    }

    /// Set the image base URL
    pub fn image_base(mut self, url: impl Into<String>) -> Self {
        self.image_base = url.into();
        self
    }

    /// Set the bitrate/quality level
    ///
    /// # Example
    ///
    /// ```
    /// # use pmoparadise::{RadioParadiseClient, Bitrate};
    /// let builder = RadioParadiseClient::builder()
    ///     .bitrate(Bitrate::Aac320);
    /// ```
    pub fn bitrate(mut self, bitrate: Bitrate) -> Self {
        self.bitrate = bitrate;
        self
    }

    /// Set the channel (0 = main mix, 1 = mellow, 2 = rock, 3 = world/etc)
    pub fn channel(mut self, channel: u8) -> Self {
        self.channel = channel;
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
    pub async fn build(self) -> Result<RadioParadiseClient> {
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

        let block_base = if self.block_base == DEFAULT_BLOCK_BASE {
            RadioParadiseClient::block_base_for_channel(self.channel)
        } else {
            self.block_base.clone()
        };

        Ok(RadioParadiseClient {
            client,
            api_base: self.api_base,
            block_base,
            image_base: self.image_base,
            bitrate: self.bitrate,
            channel: self.channel,
            timeout: self.timeout,
            next_block_url: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let builder = ClientBuilder::default();
        assert_eq!(builder.api_base, DEFAULT_API_BASE);
        assert_eq!(builder.bitrate, Bitrate::Flac);
        assert_eq!(builder.channel, 0);
    }

    #[test]
    fn test_cover_url() {
        let client = RadioParadiseClient::with_client(Client::new());
        let url = client.cover_url("test.jpg").unwrap();
        assert_eq!(
            url.as_str(),
            "https://img.radioparadise.com/covers/l/test.jpg"
        );
    }
}
