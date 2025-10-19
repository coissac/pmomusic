use anyhow::Result;
use image::{imageops::FilterType, DynamicImage};
use webp::{Encoder, WebPMemory};

pub fn encode_webp(img: &DynamicImage) -> Result<Vec<u8>> {
    let rgb_img = img.to_rgba8();
    let encoder = Encoder::from_rgba(&rgb_img, rgb_img.width(), rgb_img.height());
    let webp_data: WebPMemory = encoder.encode(85.0);
    Ok(webp_data.to_vec())
}

pub fn ensure_square(img: &DynamicImage, size: u32) -> DynamicImage {
    let (width, height) = (img.width(), img.height());

    // Calculer le ratio de mise à l'échelle
    let scale = if width > height {
        size as f32 / width as f32
    } else {
        size as f32 / height as f32
    };

    let new_width = (width as f32 * scale) as u32;
    let new_height = (height as f32 * scale) as u32;

    // Redimensionner l'image
    let resized = img.resize(new_width, new_height, FilterType::Lanczos3);

    // Créer une image carrée avec fond transparent
    let mut square = DynamicImage::new_rgba8(size, size);

    // Calculer la position pour centrer l'image redimensionnée
    let x = (size - new_width) / 2;
    let y = (size - new_height) / 2;

    // Copier l'image redimensionnée au centre du carré
    image::imageops::overlay(&mut square, &resized, x.into(), y.into());

    square
}

pub async fn generate_variant(
    cache: &super::cache::Cache,
    pk: &str,
    size: usize,
) -> Result<Vec<u8>> {
    // Utiliser file_path_with_qualifier pour obtenir le chemin
    let variant_path = cache.file_path_with_qualifier(pk, &size.to_string());

    if variant_path.exists() {
        return Ok(tokio::fs::read(variant_path).await?);
    }

    let orig_path = cache.file_path_with_qualifier(pk, "orig");

    // Charger l'image de manière synchrone (image::open n'est pas async)
    let img = tokio::task::spawn_blocking(move || image::open(orig_path)).await??;

    let square = ensure_square(&img, size as u32);
    let webp_data = encode_webp(&square)?;

    tokio::fs::write(&variant_path, &webp_data).await?;
    Ok(webp_data)
}
