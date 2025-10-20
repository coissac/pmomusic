//! Per-track extraction from FLAC blocks (optional feature)
//!
//! **Important Notes:**
//!
//! Radio Paradise publishes *blocks* containing multiple songs, not individual
//! per-track files. This module provides experimental functionality to extract
//! individual tracks from FLAC blocks, but comes with significant tradeoffs:
//!
//! - **Storage**: Requires downloading the entire block (50-100MB) to disk
//! - **Latency**: Must download and decode before playback can start
//! - **CPU**: FLAC decoding is CPU-intensive
//! - **Complexity**: Seeking in FLAC requires decoding from the beginning
//!
//! ## Recommended Alternative
//!
//! For most use cases, it's better to:
//! 1. Stream the entire block to your audio player
//! 2. Use the `song[i].elapsed` metadata to seek within the player
//! 3. Let the player handle gapless transitions between tracks
//!
//! Modern players (mpv, VLC, ffmpeg) can seek in FLAC streams efficiently.
//!
//! ## When to Use This Module
//!
//! Only use per-track extraction when you need:
//! - Individual WAV files for further processing
//! - PCM data for custom audio analysis
//! - Separate files for non-streaming scenarios
//!
//! ## Block URL Pattern
//!
//! Blocks follow this URL pattern:
//! ```text
//! https://apps.radioparadise.com/blocks/chan/0/4/<start_event>-<end_event>.flac
//! ```
//!
//! The `song[i].elapsed` field (in milliseconds) indicates when each track
//! starts within the block.

#[cfg(feature = "per-track")]
use crate::error::{Error, Result};
#[cfg(feature = "per-track")]
use crate::models::Block;
#[cfg(feature = "per-track")]
use crate::RadioParadiseClient;
#[cfg(feature = "per-track")]
use std::io::Write;
#[cfg(feature = "per-track")]
use std::path::PathBuf;

/// Metadata for a decoded track stream
#[cfg(feature = "per-track")]
#[derive(Debug, Clone)]
pub struct TrackMetadata {
    /// Sample rate in Hz (e.g., 44100)
    pub sample_rate: u32,
    /// Number of audio channels (1 = mono, 2 = stereo)
    pub channels: u16,
    /// Bits per sample (typically 16 or 24)
    pub bits_per_sample: u16,
    /// Total number of samples in this track
    pub total_samples: u64,
}

/// A stream of decoded PCM audio for a single track
///
/// Provides access to decoded FLAC audio data for one track within a block.
/// The audio is decoded to 16-bit PCM format.
#[cfg(feature = "per-track")]
pub struct TrackStream {
    /// Audio format metadata
    pub metadata: TrackMetadata,
    /// Path to the temporary FLAC file
    temp_path: PathBuf,
    /// FLAC reader
    reader: Option<claxon::FlacReader<std::io::BufReader<std::fs::File>>>,
    /// Current sample position
    current_sample: u64,
    /// End sample position (where this track ends)
    end_sample: u64,
}

#[cfg(feature = "per-track")]
impl TrackStream {
    /// Create a new track stream from a block
    ///
    /// This will:
    /// 1. Download the entire block to a temporary file
    /// 2. Open it with a FLAC decoder
    /// 3. Seek to the track's start position
    /// 4. Prepare to decode samples
    ///
    /// **Warning**: This is an expensive operation. Consider caching blocks.
    async fn from_block_internal(
        client: &RadioParadiseClient,
        block: &Block,
        track_index: usize,
    ) -> Result<Self> {
        // Validate track index
        let song = block
            .get_song(track_index)
            .ok_or(Error::InvalidIndex(track_index, block.song_count()))?;

        // Download block to temporary file
        let url = block
            .url
            .parse()
            .map_err(|e| Error::other(format!("Invalid block URL: {}", e)))?;

        let block_data = client.download_block(&url).await?;

        // Write to temp file
        let mut temp_file = tempfile::NamedTempFile::new()?;
        temp_file.write_all(&block_data)?;
        temp_file.flush()?;

        let temp_path = temp_file.into_temp_path();
        let path_buf = temp_path.to_path_buf();

        #[cfg(feature = "logging")]
        tracing::debug!("Wrote block to temp file: {:?}", path_buf);

        // Open FLAC reader
        let file = std::fs::File::open(&path_buf)?;
        let buffered = std::io::BufReader::new(file);
        let mut reader = claxon::FlacReader::new(buffered)?;

        let streaminfo = reader.streaminfo();
        let sample_rate = streaminfo.sample_rate;
        let channels = streaminfo.channels as u16;
        let bits_per_sample = streaminfo.bits_per_sample as u16;

        // Calculate start and end sample positions
        let start_sample = Self::ms_to_samples(song.elapsed, sample_rate);
        let duration_samples = Self::ms_to_samples(song.duration, sample_rate);
        let end_sample = start_sample + duration_samples;

        #[cfg(feature = "logging")]
        tracing::debug!(
            "Track {} spans samples {} to {} ({} ms to {} ms)",
            track_index,
            start_sample,
            end_sample,
            song.elapsed,
            song.elapsed + song.duration
        );

        // Seek to start position by reading and discarding samples
        // Note: FLAC doesn't support random access, so we must decode from beginning
        if start_sample > 0 {
            #[cfg(feature = "logging")]
            tracing::debug!("Seeking to sample {}", start_sample);

            Self::skip_samples(&mut reader, start_sample)?;
        }

        let metadata = TrackMetadata {
            sample_rate,
            channels,
            bits_per_sample,
            total_samples: duration_samples,
        };

        Ok(Self {
            metadata,
            temp_path: path_buf,
            reader: Some(reader),
            current_sample: start_sample,
            end_sample,
        })
    }

    /// Convert milliseconds to sample count
    fn ms_to_samples(ms: u64, sample_rate: u32) -> u64 {
        (ms * sample_rate as u64) / 1000
    }

    /// Skip samples by reading and discarding
    fn skip_samples(
        reader: &mut claxon::FlacReader<std::io::BufReader<std::fs::File>>,
        count: u64,
    ) -> Result<()> {
        let mut samples = reader.samples();
        for _ in 0..count {
            if samples.next().is_none() {
                return Err(Error::other("Unexpected end of FLAC stream while seeking"));
            }
        }
        Ok(())
    }

    /// Read decoded PCM samples
    ///
    /// Returns samples as 16-bit signed integers (i16), interleaved by channel.
    /// For stereo: [L, R, L, R, ...]. Returns None when track ends.
    pub fn read_samples(&mut self, buffer: &mut [i16]) -> Result<Option<usize>> {
        let reader = self
            .reader
            .as_mut()
            .ok_or(Error::other("TrackStream already consumed"))?;

        let mut samples_iter = reader.samples();
        let mut count = 0;

        for chunk in buffer.chunks_mut(self.metadata.channels as usize) {
            if self.current_sample >= self.end_sample {
                break;
            }

            // Read one sample per channel
            for sample_slot in chunk.iter_mut() {
                match samples_iter.next() {
                    Some(Ok(sample)) => {
                        // Claxon returns i32, convert to i16
                        *sample_slot = (sample >> (self.metadata.bits_per_sample - 16)) as i16;
                        count += 1;
                    }
                    Some(Err(e)) => {
                        return Err(Error::FlacDecode(e.to_string()));
                    }
                    None => {
                        return Ok(if count > 0 { Some(count) } else { None });
                    }
                }
            }

            self.current_sample += 1;
        }

        Ok(if count > 0 { Some(count) } else { None })
    }

    /// Export track to a WAV file
    ///
    /// Decodes the entire track and writes it as a WAV file.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # #[cfg(feature = "per-track")]
    /// # {
    /// use pmoparadise::RadioParadiseClient;
    /// use std::path::Path;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = RadioParadiseClient::new().await?;
    /// let block = client.get_block(None).await?;
    ///
    /// let mut track_stream = client.open_track_stream(&block, 0).await?;
    /// track_stream.export_wav(Path::new("track.wav"))?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    pub fn export_wav(&mut self, output_path: &std::path::Path) -> Result<()> {
        let spec = hound::WavSpec {
            channels: self.metadata.channels,
            sample_rate: self.metadata.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::create(output_path, spec)?;
        let mut buffer = vec![0i16; 8192 * self.metadata.channels as usize];

        #[cfg(feature = "logging")]
        tracing::info!("Exporting track to WAV: {:?}", output_path);

        loop {
            match self.read_samples(&mut buffer)? {
                Some(count) => {
                    for &sample in &buffer[..count] {
                        writer.write_sample(sample)?;
                    }
                }
                None => break,
            }
        }

        writer.finalize()?;

        #[cfg(feature = "logging")]
        tracing::info!("Successfully exported WAV file");

        Ok(())
    }
}

#[cfg(feature = "per-track")]
impl Drop for TrackStream {
    fn drop(&mut self) {
        // Close reader before removing temp file
        self.reader.take();

        // Clean up temporary file
        if let Err(_e) = std::fs::remove_file(&self.temp_path) {
            #[cfg(feature = "logging")]
            tracing::warn!("Failed to remove temp file {:?}: {}", self.temp_path, _e);
        }
    }
}

#[cfg(feature = "per-track")]
impl RadioParadiseClient {
    /// Open a stream for a specific track within a block
    ///
    /// **Warning**: This downloads the entire block to a temporary file
    /// and performs FLAC decoding. See module documentation for alternatives.
    ///
    /// # Arguments
    ///
    /// * `block` - The block containing the track
    /// * `track_index` - Index of the track (0-based)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # #[cfg(feature = "per-track")]
    /// # {
    /// use pmoparadise::RadioParadiseClient;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = RadioParadiseClient::new().await?;
    /// let block = client.get_block(None).await?;
    ///
    /// // Extract first track
    /// let mut track = client.open_track_stream(&block, 0).await?;
    /// println!("Track: {} Hz, {} channels",
    ///          track.metadata.sample_rate,
    ///          track.metadata.channels);
    ///
    /// // Read some samples
    /// let mut buffer = vec![0i16; 4096];
    /// if let Some(count) = track.read_samples(&mut buffer)? {
    ///     println!("Read {} samples", count);
    /// }
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    pub async fn open_track_stream(
        &self,
        block: &Block,
        track_index: usize,
    ) -> Result<TrackStream> {
        TrackStream::from_block_internal(self, block, track_index).await
    }

    /// Helper: Get track position in seconds for player-based seeking
    ///
    /// Instead of downloading and decoding, you can pass this information
    /// to your audio player for efficient seeking.
    ///
    /// Returns (start_seconds, duration_seconds)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pmoparadise::RadioParadiseClient;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = RadioParadiseClient::new().await?;
    /// let block = client.get_block(None).await?;
    ///
    /// let (start, duration) = client.track_position_seconds(&block, 1)?;
    /// println!("Track 1 starts at {}s, duration {}s", start, duration);
    /// println!("Play with: mpv --start={} --length={} {}", start, duration, block.url);
    /// # Ok(())
    /// # }
    /// ```
    pub fn track_position_seconds(&self, block: &Block, track_index: usize) -> Result<(f64, f64)> {
        let song = block
            .get_song(track_index)
            .ok_or(Error::InvalidIndex(track_index, block.song_count()))?;

        let start_secs = song.elapsed as f64 / 1000.0;
        let duration_secs = song.duration as f64 / 1000.0;

        Ok((start_secs, duration_secs))
    }
}

#[cfg(test)]
#[cfg(feature = "per-track")]
mod tests {
    use super::*;

    #[test]
    fn test_ms_to_samples() {
        assert_eq!(TrackStream::ms_to_samples(1000, 44100), 44100);
        assert_eq!(TrackStream::ms_to_samples(500, 44100), 22050);
        assert_eq!(TrackStream::ms_to_samples(0, 44100), 0);
    }
}
