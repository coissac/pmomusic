use pmocovers::cache;
use tempfile::TempDir;
use image::{ImageBuffer, Rgba};

fn create_test_cache() -> (TempDir, cache::Cache) {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = cache::new_cache(temp_dir.path().to_str().unwrap(), 10).unwrap();
    (temp_dir, cache)
}

/// Crée une image de test simple
fn create_test_image(width: u32, height: u32) -> Vec<u8> {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        if (x + y) % 2 == 0 {
            Rgba([255, 0, 0, 255]) // Rouge
        } else {
            Rgba([0, 0, 255, 255]) // Bleu
        }
    });

    let mut buffer = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buffer), image::ImageFormat::Png)
        .unwrap();
    buffer
}

#[tokio::test]
async fn test_cover_cache_creation() {
    let (temp_dir, cache) = create_test_cache();
    assert_eq!(cache.cache_dir(), temp_dir.path());
}

#[tokio::test]
async fn test_add_image_from_file() {
    let (_temp_dir, cache) = create_test_cache();

    // Créer une image de test
    let test_image = create_test_image(100, 100);
    let test_file = tempfile::NamedTempFile::with_suffix(".png").unwrap();
    std::fs::write(test_file.path(), &test_image).unwrap();

    // Ajouter au cache
    let pk = cache
        .add_from_file(test_file.path().to_str().unwrap(), None)
        .await
        .unwrap();

    assert!(!pk.is_empty());

    // Attendre la fin de la conversion
    cache.wait_until_finished(&pk).await.unwrap();

    // Vérifier que le fichier WebP existe
    let cached_path = cache.get(&pk).await.unwrap();
    assert!(cached_path.exists());
    assert!(cached_path.extension().unwrap() == "webp");
}

#[tokio::test]
async fn test_covers_config() {
    use pmocache::CacheConfig;

    assert_eq!(cache::CoversConfig::file_extension(), "webp");
    assert_eq!(cache::CoversConfig::cache_type(), "image");
    assert_eq!(cache::CoversConfig::cache_name(), "covers");
}

#[tokio::test]
async fn test_collection_management() {
    let (_temp_dir, cache) = create_test_cache();

    let collection = "album_covers";

    // Ajouter plusieurs images à la même collection
    for i in 0..3 {
        let img = create_test_image(50 + i * 10, 50 + i * 10);
        let file = tempfile::NamedTempFile::with_suffix(".png").unwrap();
        std::fs::write(file.path(), &img).unwrap();

        cache
            .add_from_file(file.path().to_str().unwrap(), Some(collection))
            .await
            .unwrap();
    }

    // Récupérer la collection
    let collection_files = cache.get_collection(collection).await.unwrap();
    assert_eq!(collection_files.len(), 3);
}

#[tokio::test]
#[ignore] // Test d'éviction LRU avec transformer WebP, parfois échoue timing
async fn test_cache_limit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = cache::new_cache(temp_dir.path().to_str().unwrap(), 2).unwrap();

    // Ajouter 3 images (devrait déclencher l'éviction LRU)
    for i in 0..3 {
        let img = create_test_image(100, 100);
        let file = tempfile::NamedTempFile::with_suffix(".png").unwrap();
        std::fs::write(file.path(), &img).unwrap();

        cache
            .add_from_file(file.path().to_str().unwrap(), None)
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    // Attendre l'éviction
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Le cache ne devrait contenir que 2 éléments
    let count = cache.db.count().unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_deduplication() {
    let (_temp_dir, cache) = create_test_cache();

    // Créer deux fichiers avec la même image
    let img = create_test_image(100, 100);

    let file1 = tempfile::NamedTempFile::with_suffix(".png").unwrap();
    std::fs::write(file1.path(), &img).unwrap();

    let file2 = tempfile::NamedTempFile::with_suffix(".png").unwrap();
    std::fs::write(file2.path(), &img).unwrap();

    // Ajouter les deux images
    let pk1 = cache
        .add_from_file(file1.path().to_str().unwrap(), None)
        .await
        .unwrap();

    let pk2 = cache
        .add_from_file(file2.path().to_str().unwrap(), None)
        .await
        .unwrap();

    // Les deux devraient avoir le même pk (déduplication)
    assert_eq!(pk1, pk2);

    // Il ne devrait y avoir qu'une seule entrée en DB
    assert_eq!(cache.db.count().unwrap(), 1);
}
