use tokio::io::AsyncReadExt;

use pmoflac::{autodetect::decode_audio_stream, encode_flac_stream, EncoderOptions, PcmFormat};

const SAMPLE_FILES: &[&str] = &[
    "test_data/1abaa2c7fb4302e20ac570e79857b700.32bits-44.1Khz.flac",
    "test_data/file_example_MP3_5MG.mp3",
    "test_data/file_example_OOG_5MG.ogg",
    "test_data/music_orig.opus",
    "test_data/music_orig.wav",
    "test_data/wood24.aiff",
];

#[tokio::test]
async fn decode_audio_stream_recognises_formats() -> Result<(), Box<dyn std::error::Error>> {
    for path in SAMPLE_FILES {
        let file = tokio::fs::File::open(path).await?;
        let mut stream = decode_audio_stream(file).await?;
        let info = stream.info().clone();
        assert!(info.sample_rate > 0);
        assert!(info.channels > 0);
        let mut pcm = Vec::new();
        stream.read_to_end(&mut pcm).await?;
        assert!(!pcm.is_empty());
        stream.wait().await?;
    }
    Ok(())
}

#[tokio::test]
async fn decode_audio_stream_transcodes_to_flac() -> Result<(), Box<dyn std::error::Error>> {
    let file = tokio::fs::File::open("test_data/file_example_MP3_5MG.mp3").await?;
    let stream = decode_audio_stream(file).await?;
    let (info, reader) = stream.into_reader();
    let format = PcmFormat {
        sample_rate: info.sample_rate,
        channels: info.channels,
        bits_per_sample: info.bits_per_sample,
    };

    let mut flac_stream = encode_flac_stream(reader, format, EncoderOptions::default()).await?;
    let mut flac_data = Vec::new();
    flac_stream.read_to_end(&mut flac_data).await?;
    assert!(flac_data.starts_with(b"fLaC"));
    flac_stream.wait().await?;
    Ok(())
}
