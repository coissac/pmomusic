use pmoaudiocache::cache;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let cache_dir = "/tmp/test_audio_cache_debug";
    std::fs::create_dir_all(cache_dir)?;

    println!("Creating cache in: {}", cache_dir);

    let cache = Arc::new(cache::new_cache(cache_dir, 100)?);

    // URL MP3 de test - petit fichier
    let test_url = "https://fr.getsamplefiles.com/download/mp3/sample-3.mp3";

    println!("\nDownloading: {}", test_url);
    let pk = cache::add_with_metadata_extraction(cache, test_url, Some("test")).await?;

    println!("\nPK: {}", pk);
    let file_path = cache.get_file_path(&pk);
    println!("File path: {}", file_path.display());

    // Vérifier le format
    let data = std::fs::read(&file_path)?;
    if data.len() >= 4 {
        let header = &data[0..4];
        if header == b"fLaC" {
            println!("✓ File is FLAC!");
        } else if header[0..3] == *b"ID3"
            || (header.len() >= 2 && header[0] == 0xFF && (header[1] & 0xE0) == 0xE0)
        {
            println!("✗ File is still MP3!");
            println!(
                "   Header: {:02X} {:02X} {:02X} {:02X}",
                header[0], header[1], header[2], header[3]
            );
        } else {
            println!("? Unknown format");
            println!(
                "   Header: {:02X} {:02X} {:02X} {:02X}",
                header[0], header[1], header[2], header[3]
            );
        }
    }

    Ok(())
}
