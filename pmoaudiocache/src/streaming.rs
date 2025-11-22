//! Audio streaming transformer built on pmoflac.
//!
//! This module wires the generic `pmocache` download pipeline with the new
//! streaming transcode helper provided by `pmoflac`. Any supported codec
//! (FLAC, MP3, OGG/Vorbis, Opus, WAV, AIFF) is converted to FLAC on the fly,
//! while native FLAC input is forwarded byte-for-byte without re-encoding.

use bytes::Bytes;
use pmocache::download::TransformMetadata;
use pmocache::StreamTransformer;
use pmoflac::{transcode_to_flac_stream, AudioCodec, TranscodeOptions};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Creates the transformer consumed by the audio cache.
pub fn create_streaming_flac_transformer() -> StreamTransformer {
    Box::new(|input, mut file, context| {
        Box::pin(async move {
            let byte_stream = input.into_byte_stream();
            let reader = StreamToAsyncRead::new(byte_stream);

            let transcode = transcode_to_flac_stream(reader, TranscodeOptions::default())
                .await
                .map_err(|e| format!("Audio transcode error: {}", e))?;

            let codec = transcode.input_codec();
            let info = transcode.input_stream_info().clone();
            log_stream_info(codec, &info);

            let mode = if transcode.is_passthrough() {
                "passthrough"
            } else {
                "transcode"
            };

            context
                .set_metadata(TransformMetadata {
                    mode: Some(mode.to_string()),
                    input_codec: Some(codec_to_string(codec)),
                    details: Some(
                        json!({
                            "sample_rate": info.sample_rate,
                            "bits_per_sample": info.bits_per_sample,
                            "channels": info.channels,
                            "total_samples": info.total_samples,
                        })
                        .to_string(),
                    ),
                    sample_rate: Some(info.sample_rate),
                    bits_per_sample: Some(info.bits_per_sample),
                    channels: Some(info.channels),
                    total_samples: info.total_samples,
                })
                .await;

            let mut flac_stream = transcode.into_stream();
            let mut buffer = vec![0u8; 64 * 1024];
            let mut total_written = 0u64;

            loop {
                let read = flac_stream
                    .read(&mut buffer)
                    .await
                    .map_err(|e| format!("Failed to read FLAC data: {}", e))?;

                if read == 0 {
                    break;
                }

                file.write_all(&buffer[..read])
                    .await
                    .map_err(|e| format!("Failed to write FLAC file: {}", e))?;

                total_written += read as u64;
                context.report_progress(total_written);
            }

            file.flush()
                .await
                .map_err(|e| format!("Failed to flush FLAC file: {}", e))?;

            flac_stream
                .wait()
                .await
                .map_err(|e| format!("FLAC encoder error: {}", e))?;

            Ok(())
        })
    })
}

fn log_stream_info(codec: AudioCodec, info: &pmoflac::StreamInfo) {
    tracing::debug!(
        "Detected codec {:?}: {} Hz, {} channels, {} bits/sample (passthrough={})",
        codec,
        info.sample_rate,
        info.channels,
        info.bits_per_sample,
        codec == AudioCodec::Flac
    );
}

fn codec_to_string(codec: AudioCodec) -> String {
    match codec {
        AudioCodec::Flac => "flac",
        AudioCodec::Mp3 => "mp3",
        AudioCodec::OggVorbis => "ogg_vorbis",
        AudioCodec::OggOpus => "ogg_opus",
        AudioCodec::Wav => "wav",
        AudioCodec::Aiff => "aiff",
    }
    .to_string()
}

/// Adapter exposing a byte stream as `AsyncRead`.
struct StreamToAsyncRead {
    stream: futures_util::stream::BoxStream<'static, Result<Bytes, String>>,
    current_chunk: Option<Bytes>,
    offset: usize,
}

impl StreamToAsyncRead {
    fn new(
        stream: std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<Bytes, String>> + Send>>,
    ) -> Self {
        use futures_util::StreamExt;
        Self {
            stream: stream.boxed(),
            current_chunk: None,
            offset: 0,
        }
    }
}

impl tokio::io::AsyncRead for StreamToAsyncRead {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use futures_util::StreamExt;
        use std::task::Poll;

        loop {
            if let Some(chunk) = &self.current_chunk {
                if self.offset < chunk.len() {
                    let available = chunk.len() - self.offset;
                    let to_copy = available.min(buf.remaining());
                    buf.put_slice(&chunk[self.offset..self.offset + to_copy]);
                    self.offset += to_copy;
                    return Poll::Ready(Ok(()));
                }
                self.current_chunk = None;
                self.offset = 0;
            }

            match self.stream.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    if chunk.is_empty() {
                        continue;
                    }
                    self.current_chunk = Some(chunk);
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, e)));
                }
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl Unpin for StreamToAsyncRead {}
