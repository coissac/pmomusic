use pmoflac::decode_aac_stream;
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn test_decode_adts_file() {
    let file = tokio::fs::File::open("/tmp/test_adts.aac")
        .await
        .expect("test ADTS file not found — run: ffmpeg -i tests/SBRtestStereoHiBr.mp4 -vn -acodec copy -f adts /tmp/test_adts.aac");

    let mut stream = decode_aac_stream(file)
        .await
        .expect("decode_aac_stream failed");

    let info = stream.info().clone();
    println!("StreamInfo: {} Hz, {} ch, {} bps", info.sample_rate, info.channels, info.bits_per_sample);

    assert!(info.sample_rate > 0, "sample_rate should be > 0");
    assert!(info.channels == 1 || info.channels == 2, "channels should be 1 or 2");
    assert_eq!(info.bits_per_sample, 16);

    let mut pcm = Vec::new();
    stream.read_to_end(&mut pcm).await.expect("read_to_end failed");

    println!("Decoded {} PCM bytes ({} samples)", pcm.len(), pcm.len() / 2);
    assert!(pcm.len() > 0, "should have decoded some PCM data");
}

#[tokio::test]
async fn test_autodetect_adts() {
    use pmoflac::decode_audio_stream;

    let file = tokio::fs::File::open("/tmp/test_adts.aac")
        .await
        .expect("test ADTS file not found");

    let stream = decode_audio_stream(file)
        .await
        .expect("decode_audio_stream failed");

    let info = stream.info().clone();
    println!("Autodetect: {} Hz, {} ch, {} bps", info.sample_rate, info.channels, info.bits_per_sample);
    assert!(info.sample_rate > 0);
}
