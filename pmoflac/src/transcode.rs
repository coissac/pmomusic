//! High-level audio transcoding helpers.
//!
//! This module exposes convenience utilities to convert any format supported by
//! `pmoflac` into a streaming FLAC pipeline. When the input is already FLAC the
//! transcoder bypasses any re-encoding and simply forwards the original byte
//! stream.

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncReadExt, BufReader, ReadBuf};

use crate::{
    autodetect::{decode_audio_stream, DecodeAudioError, DecodedAudioStream},
    encode_flac_stream,
    prefixed_reader::PrefixedReader,
    EncoderOptions, FlacEncodedStream, FlacError, PcmFormat, StreamInfo,
};

const READ_CHUNK: usize = 4096;

/// List of audio codecs supported by the transcoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Flac,
    Mp3,
    OggVorbis,
    OggOpus,
    Wav,
    Aiff,
}

/// Options controlling how the transcoder operates.
#[derive(Debug, Clone)]
pub struct TranscodeOptions {
    /// Encoder options forwarded to the FLAC backend.
    pub encoder_options: EncoderOptions,
}

impl Default for TranscodeOptions {
    fn default() -> Self {
        Self {
            encoder_options: EncoderOptions::default(),
        }
    }
}

/// Stream returned by [`transcode_to_flac_stream`].
pub struct FlacTranscodeStream {
    inner: FlacTranscodeInner,
}

enum FlacTranscodeInner {
    Passthrough(Pin<Box<dyn AsyncRead + Send>>),
    Encoded(FlacEncodedStream),
}

impl FlacTranscodeStream {
    fn passthrough<R>(reader: R) -> Self
    where
        R: AsyncRead + Send + 'static,
    {
        Self {
            inner: FlacTranscodeInner::Passthrough(Box::pin(reader)),
        }
    }

    fn encoded(stream: FlacEncodedStream) -> Self {
        Self {
            inner: FlacTranscodeInner::Encoded(stream),
        }
    }

    /// Returns true when the input was already FLAC and no re-encoding was required.
    pub fn is_passthrough(&self) -> bool {
        matches!(self.inner, FlacTranscodeInner::Passthrough(_))
    }

    /// Waits for the encoding task (if any) to finish.
    pub async fn wait(self) -> Result<(), FlacError> {
        match self.inner {
            FlacTranscodeInner::Passthrough(_) => Ok(()),
            FlacTranscodeInner::Encoded(stream) => stream.wait().await,
        }
    }
}

impl AsyncRead for FlacTranscodeStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut self.as_mut().get_mut().inner {
            FlacTranscodeInner::Passthrough(reader) => reader.as_mut().poll_read(cx, buf),
            FlacTranscodeInner::Encoded(stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

/// Result of a successful FLAC transcoding pipeline.
pub struct TranscodeToFlac {
    codec: AudioCodec,
    info: StreamInfo,
    stream: FlacTranscodeStream,
}

impl TranscodeToFlac {
    fn new(codec: AudioCodec, info: StreamInfo, stream: FlacTranscodeStream) -> Self {
        Self {
            codec,
            info,
            stream,
        }
    }

    /// Returns the codec that was detected for the input stream.
    pub fn input_codec(&self) -> AudioCodec {
        self.codec
    }

    /// Returns the PCM characteristics (or FLAC metadata when passthrough) of the input.
    pub fn input_stream_info(&self) -> &StreamInfo {
        &self.info
    }

    /// Indicates whether the transcoder forwarded the original FLAC stream unchanged.
    pub fn is_passthrough(&self) -> bool {
        self.stream.is_passthrough()
    }

    /// Consumes the result and yields the FLAC stream.
    pub fn into_stream(self) -> FlacTranscodeStream {
        self.stream
    }

    /// Consumes the result and returns the associated information with the stream.
    pub fn into_parts(self) -> (AudioCodec, StreamInfo, FlacTranscodeStream) {
        (self.codec, self.info, self.stream)
    }
}

/// Error type returned by [`transcode_to_flac_stream`].
#[derive(thiserror::Error, Debug)]
pub enum TranscodeError {
    #[error("decode error: {0}")]
    Decode(#[from] DecodeAudioError),
    #[error("encode error: {0}")]
    Encode(#[from] FlacError),
}

/// Transcodes any supported audio stream into FLAC using a fully streaming pipeline.
///
/// This function combines the automatic format detection logic with the streaming
/// decoders and the FLAC encoder. The input is never fully buffered in memory:
/// chunks are decoded and immediately re-encoded, allowing low-latency pipelines
/// suitable for HTTP downloads or on-the-fly conversions.
pub async fn transcode_to_flac_stream<R>(
    reader: R,
    options: TranscodeOptions,
) -> Result<TranscodeToFlac, TranscodeError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut reader = BufReader::new(reader);
    let mut prefix = Vec::new();

    if let Some(info) = detect_flac_passthrough(&mut reader, &mut prefix).await? {
        let passthrough_reader = PrefixedReader::new(prefix, reader);
        let stream = FlacTranscodeStream::passthrough(passthrough_reader);
        return Ok(TranscodeToFlac::new(AudioCodec::Flac, info, stream));
    }

    let prefixed_reader = PrefixedReader::new(prefix, reader);
    let decoded = decode_audio_stream(prefixed_reader).await?;
    match decoded {
        DecodedAudioStream::Flac(stream) => {
            transcode_from_decoded(AudioCodec::Flac, stream, options.encoder_options).await
        }
        DecodedAudioStream::Mp3(stream) => {
            transcode_from_decoded(AudioCodec::Mp3, stream, options.encoder_options).await
        }
        DecodedAudioStream::OggVorbis(stream) => {
            transcode_from_decoded(AudioCodec::OggVorbis, stream, options.encoder_options).await
        }
        DecodedAudioStream::OggOpus(stream) => {
            transcode_from_decoded(AudioCodec::OggOpus, stream, options.encoder_options).await
        }
        DecodedAudioStream::Wav(stream) => {
            transcode_from_decoded(AudioCodec::Wav, stream, options.encoder_options).await
        }
        DecodedAudioStream::Aiff(stream) => {
            transcode_from_decoded(AudioCodec::Aiff, stream, options.encoder_options).await
        }
    }
}

async fn transcode_from_decoded<E>(
    codec: AudioCodec,
    stream: crate::decoder_common::DecodedStream<E>,
    encoder_options: EncoderOptions,
) -> Result<TranscodeToFlac, TranscodeError>
where
    E: std::error::Error + From<String> + Send + 'static,
{
    let info = stream.info().clone();
    let pcm_format = PcmFormat {
        sample_rate: info.sample_rate,
        channels: info.channels,
        bits_per_sample: info.bits_per_sample,
    };

    let flac_stream = encode_flac_stream(stream, pcm_format, encoder_options).await?;
    Ok(TranscodeToFlac::new(
        codec,
        info,
        FlacTranscodeStream::encoded(flac_stream),
    ))
}

async fn detect_flac_passthrough<R>(
    reader: &mut BufReader<R>,
    prefix: &mut Vec<u8>,
) -> Result<Option<StreamInfo>, DecodeAudioError>
where
    R: AsyncRead + Unpin,
{
    ensure_prefix_len(reader, prefix, 4).await?;
    if &prefix[..4] != b"fLaC" {
        return Ok(None);
    }

    ensure_prefix_len(reader, prefix, 8).await?;
    let block_header = &prefix[4..8];
    let block_type = block_header[0] & 0x7F;
    let block_len = ((block_header[1] as usize) << 16)
        | ((block_header[2] as usize) << 8)
        | block_header[3] as usize;

    if block_type != 0 || block_len < 34 {
        return Ok(None);
    }

    let total_len = 8 + block_len;
    ensure_prefix_len(reader, prefix, total_len).await?;
    let stream_info_block = &prefix[8..total_len];
    Ok(parse_flac_stream_info(stream_info_block))
}

async fn ensure_prefix_len<R>(
    reader: &mut BufReader<R>,
    buf: &mut Vec<u8>,
    target_len: usize,
) -> Result<(), DecodeAudioError>
where
    R: AsyncRead + Unpin,
{
    let mut tmp = [0u8; READ_CHUNK];
    while buf.len() < target_len {
        let needed = target_len - buf.len();
        let chunk_len = tmp.len().min(needed);
        let read = reader
            .read(&mut tmp[..chunk_len])
            .await
            .map_err(DecodeAudioError::from)?;
        if read == 0 {
            return Err(DecodeAudioError::UnknownFormat);
        }
        buf.extend_from_slice(&tmp[..read]);
    }
    Ok(())
}

fn parse_flac_stream_info(block: &[u8]) -> Option<StreamInfo> {
    if block.len() < 34 {
        return None;
    }

    let min_block_size = u16::from_be_bytes([block[0], block[1]]);
    let max_block_size = u16::from_be_bytes([block[2], block[3]]);

    let sample_rate =
        ((block[10] as u32) << 12) | ((block[11] as u32) << 4) | ((block[12] as u32) >> 4);

    let channels = ((block[12] & 0x0E) >> 1) + 1;
    let bits_per_sample = ((((block[12] & 0x01) as u32) << 4) | ((block[13] as u32) >> 4)) + 1;

    let total_samples = ((block[13] as u64 & 0x0F) << 32)
        | ((block[14] as u64) << 24)
        | ((block[15] as u64) << 16)
        | ((block[16] as u64) << 8)
        | (block[17] as u64);

    Some(StreamInfo {
        sample_rate,
        channels,
        bits_per_sample: bits_per_sample as u8,
        total_samples: if total_samples == 0 {
            None
        } else {
            Some(total_samples)
        },
        max_block_size,
        min_block_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn transcode_roundtrip_flac_passthrough() -> Result<(), Box<dyn std::error::Error>> {
        let pcm_samples = vec![0i16; 44_100 * 2];
        let pcm_bytes: Vec<u8> = pcm_samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();

        let pcm_format = PcmFormat {
            sample_rate: 44_100,
            channels: 2,
            bits_per_sample: 16,
        };

        let mut source_stream = encode_flac_stream(
            Cursor::new(pcm_bytes.clone()),
            pcm_format,
            EncoderOptions::default(),
        )
        .await?;

        let mut flac_data = Vec::new();
        source_stream.read_to_end(&mut flac_data).await?;
        source_stream.wait().await?;

        let result =
            transcode_to_flac_stream(Cursor::new(flac_data.clone()), TranscodeOptions::default())
                .await?;

        assert_eq!(result.input_codec(), AudioCodec::Flac);
        assert!(result.is_passthrough());
        assert_eq!(result.input_stream_info().sample_rate, 44_100);
        assert_eq!(result.input_stream_info().channels, 2);

        let mut stream = result.into_stream();
        let mut output = Vec::new();
        stream.read_to_end(&mut output).await?;
        stream.wait().await?;

        assert_eq!(output, flac_data);
        Ok(())
    }
}
