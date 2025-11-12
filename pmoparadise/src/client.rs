//! HTTP client for Radio Paradise API

use crate::error::{Error, Result};
use crate::models::{Block, EventId, NowPlaying};
use reqwest::Client;
use std::time::Duration;
use url::Url;

/// Default Radio Paradise API base URL
pub const DEFAULT_API_BASE: &str = "https://api.radioparadise.com/api";

/// Default block base URL (channel is appended)
pub const DEFAULT_BLOCK_BASE: &str = "https://apps.radioparadise.com/blocks/chan";

/// Default image base URL
pub const DEFAULT_IMAGE_BASE: &str = "https://img.radioparadise.com/";

/// Default timeout for metadata HTTP requests
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Default timeout for large block downloads/streams
/// IMPORTANT: Radio Paradise blocks can be ~20 minutes long, and with backpressure
/// from the audio pipeline, the HTTP stream must stay open for the entire duration.
/// Setting this to 2 hours to safely handle even the longest blocks.
pub const DEFAULT_BLOCK_TIMEOUT_SECS: u64 = 7200; // 2 hours

/// Default User-Agent
pub const DEFAULT_USER_AGENT: &str = "pmoparadise/0.1.0";

/// Default channel (0 = main mix)
pub const DEFAULT_CHANNEL: u8 = 0;

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
    channel: u8,
    pub(crate) request_timeout: Duration,
    pub(crate) block_timeout: Duration,
    next_block_url: Option<String>,
}

impl RadioParadiseClient {
    /// Create a new client with default settings
    ///
    /// Uses FLAC quality and channel 0 (main mix)
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
    ///
    /// Note: Uses default settings (channel 0, default timeouts).
    /// For more control, use `ClientBuilder::default().client(client).build()`.
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            api_base: DEFAULT_API_BASE.to_string(),
            channel: DEFAULT_CHANNEL,
            request_timeout: Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
            block_timeout: Duration::from_secs(DEFAULT_BLOCK_TIMEOUT_SECS),
            next_block_url: None,
        }
    }

    /// Get the current channel (0 = main mix)
    pub fn channel(&self) -> u8 {
        self.channel
    }

    /// Get the block base URL for this client's channel
    pub fn block_base(&self) -> String {
        format!("{}/{}", DEFAULT_BLOCK_BASE, self.channel)
    }

    /// Clone the client with a different channel while preserving other settings.
    pub fn clone_with_channel(&self, channel: u8) -> Self {
        let mut cloned = self.clone();
        cloned.channel = channel;
        cloned.next_block_url = None;
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
            .append_pair("bitrate", "4") // FLAC lossless
            .append_pair("info", "true")
            .append_pair("channel", &self.channel.to_string());

        if let Some(event_id) = event {
            url.query_pairs_mut()
                .append_pair("event", &event_id.to_string());
        }

        #[cfg(feature = "logging")]
        tracing::debug!("Fetching block: {}", url);

        let response = self
            .client
            .get(url)
            .timeout(self.request_timeout)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Error::other(format!(
                "API returned error status: {}",
                response.status()
            )));
        }

        let mut block: Block = response.json().await?;

        // Normalize protocol-relative URLs from API (//img.radioparadise.com/)
        if let Some(ref base) = block.image_base {
            if base.starts_with("//") {
                block.image_base = Some(format!("https:{}", base));
            }
        } else {
            // Fallback if API doesn't provide image_base (should never happen)
            block.image_base = Some(DEFAULT_IMAGE_BASE.to_string());
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
    channel: u8,
    request_timeout: Duration,
    block_timeout: Duration,
    user_agent: String,
    proxy: Option<String>,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            client: None,
            api_base: DEFAULT_API_BASE.to_string(),
            channel: DEFAULT_CHANNEL,
            request_timeout: Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
            block_timeout: Duration::from_secs(DEFAULT_BLOCK_TIMEOUT_SECS),
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

    /// Set the channel (0 = main mix, 1 = mellow, 2 = rock, 3 = world/etc)
    pub fn channel(mut self, channel: u8) -> Self {
        self.channel = channel;
        self
    }

    /// Set the request timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Set the timeout specifically for block downloads/streams
    pub fn block_timeout(mut self, timeout: Duration) -> Self {
        self.block_timeout = timeout;
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
                .timeout(self.request_timeout);

            if let Some(proxy_url) = &self.proxy {
                let proxy = reqwest::Proxy::all(proxy_url)
                    .map_err(|e| Error::other(format!("Invalid proxy: {}", e)))?;
                builder = builder.proxy(proxy);
            }

            builder.build()?
        };

        Ok(RadioParadiseClient {
            client,
            api_base: self.api_base,
            channel: self.channel,
            request_timeout: self.request_timeout,
            block_timeout: self.block_timeout,
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
        assert_eq!(builder.channel, DEFAULT_CHANNEL);
    }
}
