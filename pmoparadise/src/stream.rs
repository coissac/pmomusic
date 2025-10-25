//! Block streaming functionality

use crate::error::{Error, Result};
use crate::models::Block;
use crate::RadioParadiseClient;
use bytes::Bytes;
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};
use url::Url;

/// A stream of audio data from a Radio Paradise block
///
/// This wraps the HTTP response body and provides a `Stream<Item = Result<Bytes>>`
/// that can be consumed by audio players or written to a file.
pub struct BlockStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>,
}

impl BlockStream {
    /// Create a new block stream from a reqwest response
    pub(crate) fn new(stream: impl Stream<Item = Result<Bytes>> + Send + 'static) -> Self {
        Self {
            inner: Box::pin(stream),
        }
    }
}

impl Stream for BlockStream {
    type Item = Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl RadioParadiseClient {
    /// Stream a block from its URL
    ///
    /// Returns a `Stream` of audio bytes that can be consumed by an audio player.
    /// The stream will continue until the entire block is downloaded or an error occurs.
    ///
    /// # Arguments
    ///
    /// * `block_url` - The URL of the block to stream
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pmoparadise::RadioParadiseClient;
    /// use futures::StreamExt;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = RadioParadiseClient::new().await?;
    ///     let block = client.get_block(None).await?;
    ///
    ///     let mut stream = client.stream_block(&block.url.parse()?).await?;
    ///
    ///     while let Some(chunk) = stream.next().await {
    ///         let bytes = chunk?;
    ///         // Write bytes to audio player or file
    ///         println!("Received {} bytes", bytes.len());
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn stream_block(&self, block_url: &Url) -> Result<BlockStream> {
        #[cfg(feature = "logging")]
        tracing::debug!("Starting block stream: {}", block_url);

        let response = self
            .client
            .get(block_url.clone())
            .timeout(self.block_timeout)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Error::other(format!(
                "Failed to stream block: HTTP {}",
                response.status()
            )));
        }

        // Convert reqwest's byte stream to our Result type
        let stream = response.bytes_stream();
        let mapped = futures::stream::StreamExt::map(stream, |result| result.map_err(Error::from));

        Ok(BlockStream::new(mapped))
    }

    /// Stream a block directly from a Block struct
    ///
    /// Convenience method that parses the URL from the block.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pmoparadise::RadioParadiseClient;
    /// use futures::StreamExt;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = RadioParadiseClient::new().await?;
    ///     let block = client.get_block(None).await?;
    ///
    ///     let mut stream = client.stream_block_from_metadata(&block).await?;
    ///
    ///     while let Some(chunk) = stream.next().await {
    ///         let bytes = chunk?;
    ///         // Process bytes...
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn stream_block_from_metadata(&self, block: &Block) -> Result<BlockStream> {
        let url = Url::parse(&block.url)?;
        self.stream_block(&url).await
    }

    /// Download an entire block as Bytes
    ///
    /// This downloads the complete block file into memory. For streaming playback,
    /// use `stream_block()` instead which is more memory efficient.
    ///
    /// # Arguments
    ///
    /// * `block_url` - The URL of the block to download
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pmoparadise::RadioParadiseClient;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = RadioParadiseClient::new().await?;
    ///     let block = client.get_block(None).await?;
    ///     let url = block.url.parse()?;
    ///     let bytes = client.download_block(&url).await?;
    ///     println!("Downloaded {} bytes", bytes.len());
    ///     Ok(())
    /// }
    /// ```
    pub async fn download_block(&self, block_url: &Url) -> Result<Bytes> {
        let mut stream = self.stream_block(block_url).await?;
        let mut data = Vec::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            data.extend_from_slice(&chunk);
        }

        Ok(Bytes::from(data))
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_stream_creation() {
        let stream = futures::stream::once(async { Ok(Bytes::from("test")) });
        let _block_stream = BlockStream::new(stream);
    }
}
