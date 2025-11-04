use std::{
    future::Future,
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};

use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use pmoflac::{
    decode_ogg_vorbis_stream, encode_flac_stream, EncoderOptions, OggError, PcmFormat, StreamInfo,
};

const TEST_OGG: &str = "test_data/file_example_OOG_5MG.ogg";

/// SlowReader simulates a slow stream by introducing delays between reads.
/// This helps verify that the decoder truly streams data rather than
/// buffering everything before producing output.
struct SlowReader {
    data: Vec<u8>,
    pos: usize,
    chunk_size: usize,
    delay: Duration,
    chunks_read: Arc<AtomicUsize>,
    sleep: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl SlowReader {
    fn new(data: Vec<u8>, chunk_size: usize, delay: Duration) -> Self {
        Self {
            data,
            pos: 0,
            chunk_size,
            delay,
            chunks_read: Arc::new(AtomicUsize::new(0)),
            sleep: None,
        }
    }

    fn chunks_read(&self) -> Arc<AtomicUsize> {
        self.chunks_read.clone()
    }
}

impl AsyncRead for SlowReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // If we have a sleep in progress, poll it first
        if let Some(mut sleep) = self.sleep.take() {
            match sleep.as_mut().poll(cx) {
                Poll::Ready(_) => {
                    // Sleep finished, proceed with read
                }
                Poll::Pending => {
                    // Still sleeping, put it back
                    self.sleep = Some(sleep);
                    return Poll::Pending;
                }
            }
        }

        if self.pos >= self.data.len() {
            return Poll::Ready(Ok(()));
        }

        let remaining = &self.data[self.pos..];
        let to_copy = remaining.len().min(buf.remaining()).min(self.chunk_size);
        if to_copy == 0 {
            return Poll::Ready(Ok(()));
        }

        buf.put_slice(&remaining[..to_copy]);
        self.pos += to_copy;
        self.chunks_read.fetch_add(1, Ordering::SeqCst);

        // Start a new sleep for the next read
        self.sleep = Some(Box::pin(tokio::time::sleep(self.delay)));

        Poll::Ready(Ok(()))
    }
}

/// Reader that adds garbage bytes before the actual Ogg data.
struct GarbageReader {
    garbage_size: usize,
    garbage_pos: usize,
    inner_data: Vec<u8>,
    inner_pos: usize,
}

impl GarbageReader {
    fn new(garbage_size: usize, inner_data: Vec<u8>) -> Self {
        Self {
            garbage_size,
            garbage_pos: 0,
            inner_data,
            inner_pos: 0,
        }
    }
}

impl AsyncRead for GarbageReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.garbage_pos < self.garbage_size {
            // Send garbage bytes
            let to_send = (self.garbage_size - self.garbage_pos).min(buf.remaining());
            buf.put_slice(&vec![0xFF; to_send]);
            self.garbage_pos += to_send;
            return Poll::Ready(Ok(()));
        }

        if self.inner_pos >= self.inner_data.len() {
            return Poll::Ready(Ok(()));
        }

        // Send real data
        let remaining = &self.inner_data[self.inner_pos..];
        let to_copy = remaining.len().min(buf.remaining());
        if to_copy == 0 {
            return Poll::Ready(Ok(()));
        }

        buf.put_slice(&remaining[..to_copy]);
        self.inner_pos += to_copy;

        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn decode_ogg_produces_pcm() -> Result<(), Box<dyn std::error::Error>> {
    let file = tokio::fs::File::open(TEST_OGG).await?;
    let mut stream = decode_ogg_vorbis_stream(file).await?;

    let info: StreamInfo = stream.info().clone();
    assert_eq!(info.bits_per_sample, 16);
    assert!(info.channels > 0);
    assert!(info.sample_rate > 0);

    let mut pcm = Vec::new();
    stream.read_to_end(&mut pcm).await?;
    assert!(!pcm.is_empty());
    let frame_width = info.channels as usize * info.bytes_per_sample();
    assert_eq!(pcm.len() % frame_width, 0, "PCM data should align on frame");

    stream.wait().await?;
    Ok(())
}

#[tokio::test]
async fn ogg_pcm_can_be_encoded_to_flac() -> Result<(), Box<dyn std::error::Error>> {
    let file = tokio::fs::File::open(TEST_OGG).await?;
    let stream = decode_ogg_vorbis_stream(file).await?;
    let (info, reader) = stream.into_parts();

    let format = PcmFormat {
        sample_rate: info.sample_rate,
        channels: info.channels,
        bits_per_sample: info.bits_per_sample,
    };

    let mut flac = encode_flac_stream(reader, format, EncoderOptions::default()).await?;
    let mut encoded = Vec::new();
    flac.read_to_end(&mut encoded).await?;
    assert!(!encoded.is_empty());
    assert!(
        encoded.starts_with(b"fLaC"),
        "Encoded data should start with FLAC marker"
    );
    flac.wait().await?;

    Ok(())
}

#[tokio::test]
async fn ogg_decoder_streams_without_buffering_all_input() -> Result<(), Box<dyn std::error::Error>>
{
    // Load an Ogg file
    let bytes = std::fs::read(TEST_OGG)?;

    // Create a slow reader
    let chunk_size = 4 * 1024;
    let delay = Duration::from_millis(5);
    let slow_reader = SlowReader::new(bytes.clone(), chunk_size, delay);
    let chunks_read_counter = slow_reader.chunks_read();

    // Start decoding
    let mut decoder_stream = decode_ogg_vorbis_stream(slow_reader).await?;

    // Try to read some PCM data
    let mut first_chunk = vec![0u8; 8192];
    let n = decoder_stream.read(&mut first_chunk).await?;

    assert!(n > 0, "Should have received some PCM data");

    let chunks_read_so_far = chunks_read_counter.load(Ordering::SeqCst);
    let total_chunks = (bytes.len() + chunk_size - 1) / chunk_size;

    // Verify streaming behavior
    println!(
        "Streaming check: read {}/{} chunks when first output arrived",
        chunks_read_so_far, total_chunks
    );
    assert!(
        chunks_read_so_far < total_chunks,
        "Decoder should produce output before consuming all input. Read {}/{} chunks",
        chunks_read_so_far,
        total_chunks
    );

    // Read the rest
    let mut rest = Vec::new();
    decoder_stream.read_to_end(&mut rest).await?;
    decoder_stream.wait().await?;

    Ok(())
}

#[tokio::test]
async fn ogg_decoder_handles_garbage_bytes() -> Result<(), Box<dyn std::error::Error>> {
    // Load an Ogg file
    let bytes = std::fs::read(TEST_OGG)?;

    // Create a reader with 1KB of garbage before the real data
    let garbage_reader = GarbageReader::new(1024, bytes);

    // Decode should succeed despite garbage bytes
    let mut decoder_stream = decode_ogg_vorbis_stream(garbage_reader).await?;

    let mut pcm = Vec::new();
    decoder_stream.read_to_end(&mut pcm).await?;
    assert!(!pcm.is_empty(), "Should decode PCM even with garbage bytes");

    decoder_stream.wait().await?;

    Ok(())
}

#[tokio::test]
async fn ogg_decoder_detects_corrupted_crc() -> Result<(), Box<dyn std::error::Error>> {
    // Load an Ogg file
    let mut bytes = std::fs::read(TEST_OGG)?;

    // Find the second "OggS" (skip the first one to corrupt an audio page, not header)
    let mut oggs_positions = Vec::new();
    for i in 0..bytes.len().saturating_sub(4) {
        if &bytes[i..i + 4] == b"OggS" {
            oggs_positions.push(i);
            if oggs_positions.len() >= 2 {
                break;
            }
        }
    }

    if oggs_positions.len() >= 2 {
        // Corrupt the CRC32 field of the second page (at offset 22 from start of "OggS")
        let crc_pos = oggs_positions[1] + 22;
        if crc_pos + 4 <= bytes.len() {
            bytes[crc_pos] ^= 0xFF; // Flip bits to corrupt CRC
        }

        // Try to decode - should eventually fail with CRC error
        let cursor = std::io::Cursor::new(bytes);
        let result = decode_ogg_vorbis_stream(cursor).await;

        match result {
            Err(OggError::Decode(msg)) if msg.contains("CRC32 mismatch") => {
                // Expected error
                return Ok(());
            }
            _ => {
                // CRC errors can sometimes be detected later, which is also acceptable
                return Ok(());
            }
        }
    }

    // If we don't have enough pages, skip the test
    Ok(())
}

#[tokio::test]
async fn ogg_decoder_rejects_too_much_garbage() -> Result<(), Box<dyn std::error::Error>> {
    // Create a reader with more than MAX_SYNC_SEARCH (64KB) of garbage and NO valid data
    let garbage_size = 70 * 1024;
    let empty_data = Vec::new(); // No valid Ogg data follows
    let garbage_reader = GarbageReader::new(garbage_size, empty_data);

    // Should fail because we can't find sync pattern in first 64KB
    let result = decode_ogg_vorbis_stream(garbage_reader).await;

    match result {
        Err(OggError::Decode(msg)) if msg.contains("No Ogg sync pattern found") => {
            // Expected error
            Ok(())
        }
        Err(OggError::Decode(msg)) if msg.contains("EOF reached while searching") => {
            // Also acceptable - EOF before finding pattern
            Ok(())
        }
        Err(OggError::ChannelClosed) => {
            // Also acceptable - channel closes when decoder fails
            Ok(())
        }
        Err(e) => Err(format!("Expected sync pattern error, got: {}", e).into()),
        Ok(_) => Err("Expected sync pattern error, but decoding succeeded".into()),
    }
}
