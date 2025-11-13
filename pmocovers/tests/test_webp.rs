use image::{DynamicImage, ImageBuffer, Rgba};
use pmocovers::webp::{encode_webp, ensure_square};

/// Crée une image de test simple
fn create_test_image(width: u32, height: u32) -> DynamicImage {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        if (x + y) % 2 == 0 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });
    DynamicImage::ImageRgba8(img)
}

#[test]
fn test_encode_webp() {
    let img = create_test_image(100, 100);
    let webp_data = encode_webp(&img);

    assert!(webp_data.is_ok());
    let data = webp_data.unwrap();
    assert!(!data.is_empty());

    // Vérifier la signature WebP (RIFF...WEBP)
    assert_eq!(&data[0..4], b"RIFF");
    assert_eq!(&data[8..12], b"WEBP");
}

#[test]
fn test_ensure_square_portrait() {
    // Image portrait (plus haute que large)
    let img = create_test_image(100, 200);
    let square = ensure_square(&img, 256);

    assert_eq!(square.width(), 256);
    assert_eq!(square.height(), 256);
}

#[test]
fn test_ensure_square_landscape() {
    // Image landscape (plus large que haute)
    let img = create_test_image(200, 100);
    let square = ensure_square(&img, 256);

    assert_eq!(square.width(), 256);
    assert_eq!(square.height(), 256);
}

#[test]
fn test_ensure_square_already_square() {
    // Image déjà carrée
    let img = create_test_image(150, 150);
    let square = ensure_square(&img, 256);

    assert_eq!(square.width(), 256);
    assert_eq!(square.height(), 256);
}

#[test]
fn test_ensure_square_small_image() {
    // Petite image qui doit être agrandie
    let img = create_test_image(50, 50);
    let square = ensure_square(&img, 256);

    assert_eq!(square.width(), 256);
    assert_eq!(square.height(), 256);
}

#[test]
fn test_ensure_square_different_sizes() {
    let img = create_test_image(100, 100);

    // Tester différentes tailles de sortie
    for size in [64, 128, 256, 512] {
        let square = ensure_square(&img, size);
        assert_eq!(square.width(), size);
        assert_eq!(square.height(), size);
    }
}

#[tokio::test]
async fn test_generate_variant() {
    use pmocovers::cache;
    use tempfile::TempDir;

    let temp_dir = tempfile::tempdir().unwrap();
    let cache = cache::new_cache(temp_dir.path().to_str().unwrap(), 10).unwrap();

    // Créer et ajouter une image
    let img = create_test_image(400, 400);
    let mut buffer = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buffer), image::ImageFormat::Png)
        .unwrap();

    let test_file = tempfile::NamedTempFile::with_suffix(".png").unwrap();
    std::fs::write(test_file.path(), &buffer).unwrap();

    let pk = cache
        .add_from_file(test_file.path().to_str().unwrap(), None)
        .await
        .unwrap();

    cache.wait_until_finished(&pk).await.unwrap();

    // Générer une variante de taille 128
    let variant_data = pmocovers::webp::generate_variant(&cache, &pk, 128)
        .await
        .unwrap();

    assert!(!variant_data.is_empty());

    // Vérifier que c'est bien du WebP
    assert_eq!(&variant_data[0..4], b"RIFF");
    assert_eq!(&variant_data[8..12], b"WEBP");

    // Vérifier que le fichier de la variante a été créé
    let variant_path = cache.get_file_path_with_qualifier(&pk, "128");
    assert!(variant_path.exists());
}

#[tokio::test]
async fn test_generate_variant_caching() {
    use pmocovers::cache;
    use tempfile::TempDir;

    let temp_dir = tempfile::tempdir().unwrap();
    let cache = cache::new_cache(temp_dir.path().to_str().unwrap(), 10).unwrap();

    // Créer et ajouter une image
    let img = create_test_image(400, 400);
    let mut buffer = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buffer), image::ImageFormat::Png)
        .unwrap();

    let test_file = tempfile::NamedTempFile::with_suffix(".png").unwrap();
    std::fs::write(test_file.path(), &buffer).unwrap();

    let pk = cache
        .add_from_file(test_file.path().to_str().unwrap(), None)
        .await
        .unwrap();

    cache.wait_until_finished(&pk).await.unwrap();

    // Générer la variante une première fois
    let variant1 = pmocovers::webp::generate_variant(&cache, &pk, 256)
        .await
        .unwrap();

    // Générer la variante une deuxième fois (devrait lire depuis le cache)
    let variant2 = pmocovers::webp::generate_variant(&cache, &pk, 256)
        .await
        .unwrap();

    // Les deux devraient être identiques
    assert_eq!(variant1, variant2);
}
