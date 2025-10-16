//! Example showing how to use the MusicSource trait
//!
//! This example demonstrates accessing source information and images
//! from different music sources (requires pmoparadise and pmoqobuz to be compiled).

use pmosource::{MusicSource, DEFAULT_IMAGE_SIZE};

// Mock implementations for demonstration
#[derive(Debug)]
struct RadioParadiseSource;

impl MusicSource for RadioParadiseSource {
    fn name(&self) -> &str {
        "Radio Paradise"
    }

    fn id(&self) -> &str {
        "radio-paradise"
    }

    fn default_image(&self) -> &[u8] {
        // This would normally be: include_bytes!("../../pmoparadise/assets/default.webp")
        // For this example, we return an empty slice
        &[]
    }
}

#[derive(Debug)]
struct QobuzSource;

impl MusicSource for QobuzSource {
    fn name(&self) -> &str {
        "Qobuz"
    }

    fn id(&self) -> &str {
        "qobuz"
    }

    fn default_image(&self) -> &[u8] {
        // This would normally be: include_bytes!("../../pmoqobuz/assets/default.webp")
        // For this example, we return an empty slice
        &[]
    }
}

fn main() {
    println!("PMOMusic Sources\n");
    println!("Standard image size: {}x{} pixels\n", DEFAULT_IMAGE_SIZE, DEFAULT_IMAGE_SIZE);

    let sources: Vec<Box<dyn MusicSource>> = vec![
        Box::new(RadioParadiseSource),
        Box::new(QobuzSource),
    ];

    for source in sources {
        println!("Source: {}", source.name());
        println!("  ID: {}", source.id());
        println!("  Image MIME: {}", source.default_image_mime_type());
        println!("  Image size: {} bytes", source.default_image().len());
        println!();
    }

    println!("Note: In a real implementation, the images would be embedded in the binary");
    println!("      and would be approximately 3-10 KB each in WebP format.");
}
