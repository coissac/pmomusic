//! Vérifie la profondeur de bit d'un fichier FLAC

use pmoflac::decode_audio_stream;
use std::path::Path;
use tokio::fs::File;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path_str = std::env::args().nth(1).expect("Usage: check_flac_bits <file.flac>");
    let path = Path::new(&path_str);

    println!("Checking: {}", path.display());
    println!();

    // decode_audio_stream pour lire StreamInfo
    let file = File::open(&path).await?;
    let stream = decode_audio_stream(file).await?;
    let info = stream.info().clone();

    println!("StreamInfo from FLAC:");
    println!("  bits_per_sample: {}", info.bits_per_sample);
    println!("  sample_rate: {}", info.sample_rate);
    println!("  channels: {}", info.channels);
    println!("  bytes_per_sample: {}", info.bytes_per_sample());

    Ok(())
}
