use pmoaudiocache::cache;
use tempfile::TempDir;

fn create_test_cache() -> (TempDir, cache::Cache) {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = cache::new_cache(temp_dir.path().to_str().unwrap(), 10).unwrap();
    (temp_dir, cache)
}

#[tokio::test]
async fn test_audio_cache_creation() {
    let (temp_dir, cache) = create_test_cache();
    assert_eq!(cache.cache_dir(), temp_dir.path());
}

#[tokio::test]
async fn test_add_from_file() {
    let (_temp_dir, cache) = create_test_cache();

    // Créer un fichier FLAC de test (vide pour le moment)
    let test_file = tempfile::NamedTempFile::with_suffix(".flac").unwrap();
    // Note: Pour un test complet, il faudrait un vrai fichier FLAC avec métadonnées
    // Ici on teste juste l'ajout de fichier basique
    std::fs::write(test_file.path(), b"FLAC_DUMMY_DATA").unwrap();

    let pk = cache
        .add_from_file(test_file.path().to_str().unwrap(), None)
        .await
        .unwrap();

    assert!(!pk.is_empty());
}

#[tokio::test]
async fn test_audio_config() {
    use pmocache::CacheConfig;

    assert_eq!(cache::AudioConfig::file_extension(), "flac");
    assert_eq!(cache::AudioConfig::cache_type(), "flac");
    assert_eq!(cache::AudioConfig::cache_name(), "audio");
    assert_eq!(cache::AudioConfig::default_param(), "orig");
}

#[tokio::test]
async fn test_collection_management() {
    let (_temp_dir, cache) = create_test_cache();

    let collection = "test_album";

    // Ajouter plusieurs pistes à la même collection
    for i in 0..3 {
        let data = format!("Track {} audio data", i);
        let file = tempfile::NamedTempFile::with_suffix(".flac").unwrap();
        std::fs::write(file.path(), data.as_bytes()).unwrap();

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
async fn test_cache_limit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = cache::new_cache(temp_dir.path().to_str().unwrap(), 2).unwrap();

    // Ajouter 3 fichiers (devrait déclencher l'éviction LRU)
    for i in 0..3 {
        let data = format!("Track {}", i);
        let file = tempfile::NamedTempFile::with_suffix(".flac").unwrap();
        std::fs::write(file.path(), data.as_bytes()).unwrap();

        cache
            .add_from_file(file.path().to_str().unwrap(), None)
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // Le cache ne devrait contenir que 2 éléments
    let count = cache.db.count().unwrap();
    assert_eq!(count, 2);
}
