//! Example showing how to access and save the Qobuz source image
//!
//! This example demonstrates:
//! - Getting source information via the MusicSource trait
//! - Accessing the embedded WebP image
//! - Optionally saving it to a file

use pmoaudiocache::cache as audio_cache;
use pmocovers::cache as cover_cache;
use pmoqobuz::{QobuzClient, QobuzSource};
use pmosource::MusicSource;
use std::fs;
use std::io::Write;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create the client and source
    let client = QobuzClient::from_config().await?;
    let temp_dir = std::env::temp_dir().join("pmoqobuz_show_source_image");
    let covers_dir = temp_dir.join("covers");
    let audio_dir = temp_dir.join("audio");
    fs::create_dir_all(&covers_dir)?;
    fs::create_dir_all(&audio_dir)?;

    let cover_cache = Arc::new(cover_cache::new_cache(&covers_dir.to_string_lossy(), 128)?);
    let audio_cache = Arc::new(audio_cache::new_cache(&audio_dir.to_string_lossy(), 32)?);
    let source = QobuzSource::new(client, cover_cache, audio_cache);

    // Display source information
    println!("Music Source Information");
    println!("========================");
    println!("Name: {}", source.name());
    println!("ID: {}", source.id());
    println!("Image MIME type: {}", source.default_image_mime_type());

    // Get the embedded image
    let image_data = source.default_image();
    println!("Embedded image size: {} bytes", image_data.len());

    // Verify WebP format
    if image_data.len() >= 12 {
        let is_webp = &image_data[0..4] == b"RIFF" && &image_data[8..12] == b"WEBP";
        println!("Valid WebP format: {}", is_webp);
    }

    // Optional: save to file
    if std::env::args().any(|arg| arg == "--save") {
        let filename = format!("{}_default.webp", source.id());
        let mut file = fs::File::create(&filename)?;
        file.write_all(image_data)?;
        println!("\nImage saved to: {}", filename);
        println!("You can view it with: open {}", filename);
    } else {
        println!("\nTo save the image to disk, run with: --save");
    }

    Ok(())
}
