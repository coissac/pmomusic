//! Example showing how to access and save the Radio Paradise source image
//!
//! This example demonstrates:
//! - Getting source information via the MusicSource trait
//! - Accessing the embedded WebP image
//! - Optionally saving it to a file

use pmoparadise::{RadioParadiseSource, RadioParadiseClient};
use pmosource::MusicSource;
use std::fs;
use std::io::Write;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create the client and source
    let client = RadioParadiseClient::new().await?;
    let source = RadioParadiseSource::new_default(client, "http://localhost:8080");

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
