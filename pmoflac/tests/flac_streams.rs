use std::{
    io,
    path::PathBuf,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use pmoflac::{decode_flac_stream, encode_flac_stream, EncoderOptions, FlacError, PcmFormat};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test_data")
        .join(name)
}

struct VecAsyncReader {
    data: Vec<u8>,
    pos: usize,
}

impl VecAsyncReader {
    fn new(data: Vec<u8>) -> Self {
        Self { data, pos: 0 }
    }
}

impl AsyncRead for VecAsyncReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.pos >= self.data.len() {
            return Poll::Ready(Ok(()));
        }
        let remaining = &self.data[self.pos..];
        let to_copy = remaining.len().min(buf.remaining());
        if to_copy == 0 {
            return Poll::Ready(Ok(()));
        }
        buf.put_slice(&remaining[..to_copy]);
        self.pos += to_copy;
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn decode_stream_info_16_44() -> Result<(), FlacError> {
    let bytes = std::fs::read(fixture(
        "1abaa2c7fb4302e20ac570e79857b700.32bits-44.1Khz.flac",
    ))?;
    let mut stream = decode_flac_stream(VecAsyncReader::new(bytes)).await?;
    let info = stream.info().clone();

    assert_eq!(info.sample_rate, 44_100);
    assert_eq!(info.channels, 2);
    assert_eq!(info.bits_per_sample, 16);

    let mut pcm = Vec::new();
    stream.read_to_end(&mut pcm).await?;
    assert!(!pcm.is_empty());
    stream.wait().await?;

    Ok(())
}

#[tokio::test]
async fn roundtrip_encode_decode_24_192() -> Result<(), FlacError> {
    let bytes = std::fs::read(fixture("Yuri-Korzunov_Movement_24bit-192kHz.flac"))?;
    let mut decoder = decode_flac_stream(VecAsyncReader::new(bytes)).await?;
    let info = decoder.info().clone();

    let mut pcm_bytes = Vec::new();
    decoder.read_to_end(&mut pcm_bytes).await?;
    decoder.wait().await?;

    assert!(!pcm_bytes.is_empty());
    assert_eq!(
        pcm_bytes.len() % (info.bytes_per_sample() * info.channels as usize),
        0
    );

    let format = PcmFormat {
        sample_rate: info.sample_rate,
        channels: info.channels,
        bits_per_sample: info.bits_per_sample,
    };
    let options = EncoderOptions {
        total_samples: info.total_samples,
        ..Default::default()
    };

    let pcm_reader = VecAsyncReader::new(pcm_bytes.clone());
    let mut encoder_stream = encode_flac_stream(pcm_reader, format, options).await?;
    let mut encoded_bytes = Vec::new();
    encoder_stream.read_to_end(&mut encoded_bytes).await?;
    encoder_stream.wait().await?;

    assert!(!encoded_bytes.is_empty());

    let flac_reader = VecAsyncReader::new(encoded_bytes);
    let mut decoder_roundtrip = decode_flac_stream(flac_reader).await?;
    let mut pcm_roundtrip = Vec::new();
    decoder_roundtrip.read_to_end(&mut pcm_roundtrip).await?;
    decoder_roundtrip.wait().await?;

    assert_eq!(pcm_roundtrip, pcm_bytes);

    Ok(())
}
