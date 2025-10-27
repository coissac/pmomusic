use tokio::io::AsyncReadExt;

use pmoflac::{
    decode_ogg_vorbis_stream, encode_flac_stream, EncoderOptions, PcmFormat, StreamInfo,
};

const TEST_OGG: &str = "test_data/file_example_OOG_5MG.ogg";

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
