use anyhow::Result;
use image::{imageops::FilterType, DynamicImage};
use webp::{Encoder, WebPMemory};

/// Encode une image en format WebP avec un niveau de qualité de 85%
///
/// # Arguments
///
/// * `img` - Image à encoder
///
/// # Returns
///
/// Les données WebP encodées sous forme de vecteur d'octets
///
/// # Exemple
///
/// ```rust,ignore
/// use pmocovers::webp::encode_webp;
/// use image::DynamicImage;
///
/// let img = DynamicImage::new_rgba8(100, 100);
/// let webp_data = encode_webp(&img)?;
/// ```
pub fn encode_webp(img: &DynamicImage) -> Result<Vec<u8>> {
    let rgb_img = img.to_rgba8();
    let encoder = Encoder::from_rgba(&rgb_img, rgb_img.width(), rgb_img.height());
    let webp_data: WebPMemory = encoder.encode(85.0);
    Ok(webp_data.to_vec())
}

/// Redimensionne une image pour l'inscrire dans un carré de taille donnée
///
/// Cette fonction préserve le ratio d'aspect de l'image originale en la redimensionnant
/// pour qu'elle tienne dans un carré, puis la centre sur un fond transparent.
///
/// # Arguments
///
/// * `img` - Image à redimensionner
/// * `size` - Taille du carré de sortie (en pixels)
///
/// # Returns
///
/// Une nouvelle image carrée de taille `size × size` avec l'image originale centrée
///
/// # Exemple
///
/// ```rust,ignore
/// use pmocovers::webp::ensure_square;
/// use image::DynamicImage;
///
/// let img = DynamicImage::new_rgba8(800, 600);
/// let square = ensure_square(&img, 256);
/// assert_eq!(square.width(), 256);
/// assert_eq!(square.height(), 256);
/// ```
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

/// Génère une variante redimensionnée d'une image en cache
///
/// Cette fonction crée (ou récupère si déjà existante) une variante redimensionnée
/// d'une image. La variante est mise en cache sur disque pour éviter les
/// recalculs futurs.
///
/// # Arguments
///
/// * `cache` - Instance du cache de couvertures
/// * `pk` - Clé primaire de l'image
/// * `size` - Taille de la variante (carré de `size × size`)
///
/// # Returns
///
/// Les données WebP de la variante redimensionnée
///
/// # Comportement
///
/// 1. Si la variante existe déjà sur disque, elle est retournée directement
/// 2. Sinon, l'image originale est chargée, redimensionnée et encodée en WebP
/// 3. La variante est sauvegardée sur disque pour utilisation future
///
/// # Exemple
///
/// ```rust,ignore
/// use pmocovers::webp::generate_variant;
///
/// let cache = pmocovers::cache::new_cache("./cache", 1000)?;
/// let variant_256 = generate_variant(&cache, "abc123", 256).await?;
/// ```
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
