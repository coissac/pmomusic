use std::{
    cmp, io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use crate::{
    decode_aiff_stream, decode_flac_stream, decode_mp3_stream, decode_ogg_opus_stream,
    decode_ogg_vorbis_stream, decode_wav_stream, pcm::StreamInfo, AiffDecodedStream, AiffError,
    FlacDecodedStream, FlacError, Mp3DecodedStream, Mp3Error, OggDecodedStream, OggError,
    OggOpusDecodedStream, OggOpusError, WavDecodedStream, WavError,
};

const MAX_SNIFF_BYTES: usize = 64 * 1024;
const READ_CHUNK: usize = 4096;

#[derive(thiserror::Error, Debug)]
pub enum DecodeAudioError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("unknown or unsupported audio format")]
    UnknownFormat,
    #[error("FLAC decode error: {0}")]
    Flac(#[from] FlacError),
    #[error("MP3 decode error: {0}")]
    Mp3(#[from] Mp3Error),
    #[error("Ogg/Vorbis decode error: {0}")]
    Vorbis(#[from] OggError),
    #[error("Ogg/Opus decode error: {0}")]
    Opus(#[from] OggOpusError),
    #[error("WAV decode error: {0}")]
    Wav(#[from] WavError),
    #[error("AIFF decode error: {0}")]
    Aiff(#[from] AiffError),
}

pub async fn decode_audio_stream<R>(reader: R) -> Result<DecodedAudioStream, DecodeAudioError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut reader = reader;
    let mut initial = Vec::new();
    let mut tmp = vec![0u8; READ_CHUNK];
    let mut detected = detect_format(&initial);

    while detected.is_none() && initial.len() < MAX_SNIFF_BYTES {
        let read = reader.read(&mut tmp).await?;
        if read == 0 {
            break;
        }
        initial.extend_from_slice(&tmp[..read]);
        detected = detect_format(&initial);
    }

    let format = detected.ok_or(DecodeAudioError::UnknownFormat)?;
    let prefixed = PrefixedReader::new(initial, reader);

    let stream = match format {
        DetectedFormat::Flac => {
            let stream = decode_flac_stream(prefixed).await?;
            DecodedAudioStream::Flac(stream)
        }
        DetectedFormat::Mp3 => {
            let stream = decode_mp3_stream(prefixed).await?;
            DecodedAudioStream::Mp3(stream)
        }
        DetectedFormat::OggVorbis => {
            let stream = decode_ogg_vorbis_stream(prefixed).await?;
            DecodedAudioStream::OggVorbis(stream)
        }
        DetectedFormat::OggOpus => {
            let stream = decode_ogg_opus_stream(prefixed).await?;
            DecodedAudioStream::OggOpus(stream)
        }
        DetectedFormat::Wav => {
            let stream = decode_wav_stream(prefixed).await?;
            DecodedAudioStream::Wav(stream)
        }
        DetectedFormat::Aiff => {
            let stream = decode_aiff_stream(prefixed).await?;
            DecodedAudioStream::Aiff(stream)
        }
    };

    Ok(stream)
}

pub enum DecodedAudioStream {
    Flac(FlacDecodedStream),
    Mp3(Mp3DecodedStream),
    OggVorbis(OggDecodedStream),
    OggOpus(OggOpusDecodedStream),
    Wav(WavDecodedStream),
    Aiff(AiffDecodedStream),
}

impl DecodedAudioStream {
    pub fn info(&self) -> &StreamInfo {
        match self {
            DecodedAudioStream::Flac(inner) => inner.info(),
            DecodedAudioStream::Mp3(inner) => inner.info(),
            DecodedAudioStream::OggVorbis(inner) => inner.info(),
            DecodedAudioStream::OggOpus(inner) => inner.info(),
            DecodedAudioStream::Wav(inner) => inner.info(),
            DecodedAudioStream::Aiff(inner) => inner.info(),
        }
    }

    pub async fn wait(self) -> Result<(), DecodeAudioError> {
        match self {
            DecodedAudioStream::Flac(inner) => inner.wait().await.map_err(DecodeAudioError::from),
            DecodedAudioStream::Mp3(inner) => inner.wait().await.map_err(DecodeAudioError::from),
            DecodedAudioStream::OggVorbis(inner) => {
                inner.wait().await.map_err(DecodeAudioError::from)
            }
            DecodedAudioStream::OggOpus(inner) => {
                inner.wait().await.map_err(DecodeAudioError::from)
            }
            DecodedAudioStream::Wav(inner) => inner.wait().await.map_err(DecodeAudioError::from),
            DecodedAudioStream::Aiff(inner) => inner.wait().await.map_err(DecodeAudioError::from),
        }
    }

    pub fn into_reader(self) -> (StreamInfo, DecodedReader) {
        match self {
            DecodedAudioStream::Flac(inner) => {
                let (info, reader) = inner.into_parts();
                (info, DecodedReader::Flac(reader))
            }
            DecodedAudioStream::Mp3(inner) => {
                let (info, reader) = inner.into_parts();
                (info, DecodedReader::Mp3(reader))
            }
            DecodedAudioStream::OggVorbis(inner) => {
                let (info, reader) = inner.into_parts();
                (info, DecodedReader::OggVorbis(reader))
            }
            DecodedAudioStream::OggOpus(inner) => {
                let (info, reader) = inner.into_parts();
                (info, DecodedReader::OggOpus(reader))
            }
            DecodedAudioStream::Wav(inner) => {
                let (info, reader) = inner.into_parts();
                (info, DecodedReader::Wav(reader))
            }
            DecodedAudioStream::Aiff(inner) => {
                let (info, reader) = inner.into_parts();
                (info, DecodedReader::Aiff(reader))
            }
        }
    }
}

impl AsyncRead for DecodedAudioStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            DecodedAudioStream::Flac(inner) => Pin::new(inner).poll_read(cx, buf),
            DecodedAudioStream::Mp3(inner) => Pin::new(inner).poll_read(cx, buf),
            DecodedAudioStream::OggVorbis(inner) => Pin::new(inner).poll_read(cx, buf),
            DecodedAudioStream::OggOpus(inner) => Pin::new(inner).poll_read(cx, buf),
            DecodedAudioStream::Wav(inner) => Pin::new(inner).poll_read(cx, buf),
            DecodedAudioStream::Aiff(inner) => Pin::new(inner).poll_read(cx, buf),
        }
    }
}

pub enum DecodedReader {
    Flac(crate::stream::ManagedAsyncReader<FlacError>),
    Mp3(crate::stream::ManagedAsyncReader<Mp3Error>),
    OggVorbis(crate::stream::ManagedAsyncReader<OggError>),
    OggOpus(crate::stream::ManagedAsyncReader<OggOpusError>),
    Wav(crate::stream::ManagedAsyncReader<WavError>),
    Aiff(crate::stream::ManagedAsyncReader<AiffError>),
}

impl DecodedReader {
    pub async fn wait(self) -> Result<(), DecodeAudioError> {
        match self {
            DecodedReader::Flac(inner) => inner.wait().await.map_err(DecodeAudioError::from),
            DecodedReader::Mp3(inner) => inner.wait().await.map_err(DecodeAudioError::from),
            DecodedReader::OggVorbis(inner) => inner.wait().await.map_err(DecodeAudioError::from),
            DecodedReader::OggOpus(inner) => inner.wait().await.map_err(DecodeAudioError::from),
            DecodedReader::Wav(inner) => inner.wait().await.map_err(DecodeAudioError::from),
            DecodedReader::Aiff(inner) => inner.wait().await.map_err(DecodeAudioError::from),
        }
    }
}

impl AsyncRead for DecodedReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            DecodedReader::Flac(inner) => Pin::new(inner).poll_read(cx, buf),
            DecodedReader::Mp3(inner) => Pin::new(inner).poll_read(cx, buf),
            DecodedReader::OggVorbis(inner) => Pin::new(inner).poll_read(cx, buf),
            DecodedReader::OggOpus(inner) => Pin::new(inner).poll_read(cx, buf),
            DecodedReader::Wav(inner) => Pin::new(inner).poll_read(cx, buf),
            DecodedReader::Aiff(inner) => Pin::new(inner).poll_read(cx, buf),
        }
    }
}

fn detect_format(bytes: &[u8]) -> Option<DetectedFormat> {
    if bytes.len() >= 4 && &bytes[..4] == b"fLaC" {
        return Some(DetectedFormat::Flac);
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return Some(DetectedFormat::Wav);
    }
    if bytes.len() >= 12
        && &bytes[..4] == b"FORM"
        && (&bytes[8..12] == b"AIFF" || &bytes[8..12] == b"AIFC")
    {
        return Some(DetectedFormat::Aiff);
    }
    if let Some(fmt) = detect_ogg(bytes) {
        return Some(fmt);
    }
    if is_mp3(bytes) {
        return Some(DetectedFormat::Mp3);
    }
    None
}

fn detect_ogg(bytes: &[u8]) -> Option<DetectedFormat> {
    if bytes.len() < 27 || &bytes[..4] != b"OggS" {
        return None;
    }
    let segment_count = bytes[26] as usize;
    let header_len = 27 + segment_count;
    if bytes.len() < header_len {
        return None;
    }
    let mut packet_len = 0usize;
    for lace in &bytes[27..27 + segment_count] {
        packet_len += *lace as usize;
        if *lace < 255 {
            break;
        }
    }
    if bytes.len() < header_len + packet_len {
        return None;
    }
    let packet = &bytes[header_len..header_len + packet_len];
    if packet.starts_with(b"OpusHead") {
        Some(DetectedFormat::OggOpus)
    } else if packet.starts_with(b"\x01vorbis") {
        Some(DetectedFormat::OggVorbis)
    } else {
        None
    }
}

fn is_mp3(bytes: &[u8]) -> bool {
    if bytes.len() >= 3 && &bytes[..3] == b"ID3" {
        return true;
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 {
        return true;
    }
    false
}

enum DetectedFormat {
    Flac,
    Mp3,
    OggVorbis,
    OggOpus,
    Wav,
    Aiff,
}

struct PrefixedReader<R> {
    prefix: Vec<u8>,
    position: usize,
    reader: R,
}

impl<R> PrefixedReader<R> {
    fn new(prefix: Vec<u8>, reader: R) -> Self {
        Self {
            prefix,
            position: 0,
            reader,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for PrefixedReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.position < self.prefix.len() && buf.remaining() > 0 {
            let remaining = self.prefix.len() - self.position;
            let to_copy = cmp::min(remaining, buf.remaining());
            buf.put_slice(&self.prefix[self.position..self.position + to_copy]);
            self.position += to_copy;
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl<R: Unpin> Unpin for PrefixedReader<R> {}
unsafe impl<R: Send> Send for PrefixedReader<R> {}
