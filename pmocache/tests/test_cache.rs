use pmocache::{Cache, CacheConfig};
use tempfile::TempDir;

/// Configuration de test simple
struct TestConfig;

impl CacheConfig for TestConfig {
    fn file_extension() -> &'static str {
        "dat"
    }

    fn cache_type() -> &'static str {
        "test"
    }

    fn cache_name() -> &'static str {
        "testcache"
    }
}

type TestCache = Cache<TestConfig>;

fn create_test_cache(limit: usize) -> (TempDir, TestCache) {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = TestCache::new(temp_dir.path().to_str().unwrap(), limit).unwrap();
    (temp_dir, cache)
}

#[tokio::test]
async fn test_cache_creation() {
    let (temp_dir, cache) = create_test_cache(10);
    assert_eq!(cache.cache_dir(), temp_dir.path());
}

#[tokio::test]
async fn test_add_from_file() {
    let (_temp_dir, cache) = create_test_cache(10);

    // Créer un fichier temporaire pour le test
    let test_file = tempfile::NamedTempFile::new().unwrap();
    let test_data = b"Hello, World! This is test data.";
    std::fs::write(test_file.path(), test_data).unwrap();

    // Ajouter le fichier au cache
    let pk = cache
        .add_from_file(test_file.path().to_str().unwrap(), None)
        .await
        .unwrap();

    // Vérifier que le fichier est dans le cache
    assert!(!pk.is_empty());
    let cached_path = cache.get(&pk).await.unwrap();
    assert!(cached_path.exists());

    // Vérifier le contenu
    let cached_data = std::fs::read(&cached_path).unwrap();
    assert_eq!(&cached_data, test_data);
}

#[tokio::test]
async fn test_add_from_reader() {
    let (_temp_dir, cache) = create_test_cache(10);

    let test_data = b"Test data from reader";
    let reader = std::io::Cursor::new(test_data.to_vec());

    // Ajouter depuis un reader
    let pk = cache
        .add_from_reader(None, reader, Some(test_data.len() as u64), None)
        .await
        .unwrap();

    // Attendre que le téléchargement soit terminé
    cache.wait_until_finished(&pk).await.unwrap();

    // Vérifier que le fichier est dans le cache
    let cached_path = cache.get(&pk).await.unwrap();
    assert!(cached_path.exists());

    // Vérifier le contenu
    let cached_data = std::fs::read(&cached_path).unwrap();
    assert_eq!(&cached_data, test_data);
}

#[tokio::test]
async fn test_cache_deduplication() {
    let (_temp_dir, cache) = create_test_cache(10);

    // Créer deux fichiers avec le même contenu
    let test_data = b"Same content for both files";

    let file1 = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file1.path(), test_data).unwrap();

    let file2 = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file2.path(), test_data).unwrap();

    // Ajouter les deux fichiers
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

#[tokio::test]
async fn test_cache_collection() {
    let (_temp_dir, cache) = create_test_cache(10);

    let collection = "test_album";

    // Ajouter plusieurs fichiers à la même collection
    let mut pks = Vec::new();
    for i in 0..3 {
        let data = format!("Track {} data", i);
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), data.as_bytes()).unwrap();

        let pk = cache
            .add_from_file(file.path().to_str().unwrap(), Some(collection))
            .await
            .unwrap();
        pks.push(pk);
    }

    // Récupérer tous les fichiers de la collection
    let collection_files = cache.get_collection(collection).await.unwrap();

    assert_eq!(collection_files.len(), 3);
}

#[tokio::test]
async fn test_delete_item() {
    let (_temp_dir, cache) = create_test_cache(10);

    // Ajouter un fichier
    let test_data = b"Data to be deleted";
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), test_data).unwrap();

    let pk = cache
        .add_from_file(file.path().to_str().unwrap(), None)
        .await
        .unwrap();

    // Vérifier qu'il existe
    assert!(cache.get(&pk).await.is_ok());

    // Supprimer
    cache.delete_item(&pk).await.unwrap();

    // Vérifier qu'il n'existe plus
    assert!(cache.get(&pk).await.is_err());
}

#[tokio::test]
async fn test_delete_collection() {
    let (_temp_dir, cache) = create_test_cache(10);

    let collection = "test_collection_delete";

    // Ajouter plusieurs fichiers
    for i in 0..3 {
        let data = format!("Item {}", i);
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), data.as_bytes()).unwrap();

        cache
            .add_from_file(file.path().to_str().unwrap(), Some(collection))
            .await
            .unwrap();
    }

    // Vérifier que la collection existe
    let collection_files = cache.get_collection(collection).await.unwrap();
    assert_eq!(collection_files.len(), 3);

    // Supprimer la collection
    cache.delete_collection(collection).await.unwrap();

    // Vérifier que la collection est vide
    let collection_files = cache.get_collection(collection).await.unwrap();
    assert_eq!(collection_files.len(), 0);
}

#[tokio::test]
async fn test_lru_eviction() {
    // Créer un cache avec une limite de 3 éléments
    let (_temp_dir, cache) = create_test_cache(3);

    let mut pks = Vec::new();

    // Ajouter 5 fichiers (devrait déclencher l'éviction)
    for i in 0..5 {
        let data = format!("File {} data", i);
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), data.as_bytes()).unwrap();

        let pk = cache
            .add_from_file(file.path().to_str().unwrap(), None)
            .await
            .unwrap();
        pks.push(pk);

        // Petit délai pour s'assurer que les timestamps sont différents
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // Le cache ne devrait contenir que 3 éléments (les plus récents)
    let count = cache.db.count().unwrap();
    assert_eq!(count, 3);

    // Les 2 premiers fichiers devraient avoir été évincés
    assert!(cache.get(&pks[0]).await.is_err());
    assert!(cache.get(&pks[1]).await.is_err());

    // Les 3 derniers devraient être présents
    assert!(cache.get(&pks[2]).await.is_ok());
    assert!(cache.get(&pks[3]).await.is_ok());
    assert!(cache.get(&pks[4]).await.is_ok());
}

#[tokio::test]
async fn test_cache_purge() {
    let (_temp_dir, cache) = create_test_cache(10);

    // Ajouter plusieurs fichiers
    for i in 0..3 {
        let data = format!("File {}", i);
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), data.as_bytes()).unwrap();

        cache
            .add_from_file(file.path().to_str().unwrap(), None)
            .await
            .unwrap();
    }

    assert_eq!(cache.db.count().unwrap(), 3);

    // Purger le cache
    cache.purge().await.unwrap();

    // Le cache devrait être vide
    assert_eq!(cache.db.count().unwrap(), 0);
}

#[tokio::test]
async fn test_get_metadata() {
    let (_temp_dir, cache) = create_test_cache(10);

    let test_data = b"Test data";
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), test_data).unwrap();

    let pk = cache
        .add_from_file(file.path().to_str().unwrap(), None)
        .await
        .unwrap();

    // Ajouter des métadonnées
    cache
        .db
        .set_a_metadata(&pk, "test_key", serde_json::json!("test_value"))
        .unwrap();

    // Récupérer les métadonnées
    let value = cache.get_a_metadata(&pk, "test_key").await.unwrap();
    assert_eq!(value, Some(serde_json::json!("test_value")));
}

#[tokio::test]
async fn test_touch() {
    let (_temp_dir, cache) = create_test_cache(10);

    let test_data = b"Test data";
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), test_data).unwrap();

    let pk = cache
        .add_from_file(file.path().to_str().unwrap(), None)
        .await
        .unwrap();

    let entry_before = cache.db.get(&pk, false).unwrap();
    let hits_before = entry_before.hits;

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Touch le fichier
    cache.touch(&pk).await.unwrap();

    let entry_after = cache.db.get(&pk, false).unwrap();
    assert_eq!(entry_after.hits, hits_before + 1);
}

#[tokio::test]
async fn test_consolidate() {
    let (_temp_dir, cache) = create_test_cache(10);

    // Ajouter un fichier
    let test_data = b"Test data";
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), test_data).unwrap();

    let pk = cache
        .add_from_file(file.path().to_str().unwrap(), None)
        .await
        .unwrap();

    // Supprimer manuellement le fichier (créer un orphelin)
    let file_path = cache.get_file_path(&pk);
    std::fs::remove_file(&file_path).unwrap();

    // Consolider devrait supprimer l'entrée orpheline de la DB
    cache.consolidate().await.unwrap();

    // L'entrée ne devrait plus exister en DB
    assert!(cache.db.get(&pk, false).is_err());
}

#[tokio::test]
async fn test_prebuffer_size() {
    let (_temp_dir, mut cache) = create_test_cache(10);

    // Configurer la taille de prébuffering
    let prebuffer_size = 1024;
    cache.set_prebuffer_size(prebuffer_size);

    assert_eq!(cache.get_prebuffer_size(), prebuffer_size);
}

#[tokio::test]
async fn test_is_finished() {
    let (_temp_dir, cache) = create_test_cache(10);

    let test_data = b"Small test data";
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), test_data).unwrap();

    let pk = cache
        .add_from_file(file.path().to_str().unwrap(), None)
        .await
        .unwrap();

    // Attendre que le téléchargement soit terminé
    cache.wait_until_finished(&pk).await.unwrap();

    // Vérifier qu'il est bien terminé
    assert!(cache.is_finished(&pk).await);
}
