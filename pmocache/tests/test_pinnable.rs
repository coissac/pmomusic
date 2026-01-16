use chrono::{Duration, Utc};
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

async fn add_test_file(cache: &TestCache, content: &str) -> String {
    let test_file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(test_file.path(), content.as_bytes()).unwrap();
    cache
        .add_from_file(test_file.path().to_str().unwrap(), None)
        .await
        .unwrap()
}

#[tokio::test]
async fn test_pin_unpin() {
    let (_temp_dir, cache) = create_test_cache(10);

    let pk = add_test_file(&cache, "Test data").await;

    // Vérifier que l'item n'est pas épinglé par défaut
    assert!(!cache.is_pinned(&pk).await.unwrap());

    // Épingler l'item
    cache.pin(&pk).await.unwrap();
    assert!(cache.is_pinned(&pk).await.unwrap());

    // Désépingler l'item
    cache.unpin(&pk).await.unwrap();
    assert!(!cache.is_pinned(&pk).await.unwrap());
}

#[tokio::test]
async fn test_pinned_excluded_from_lru() {
    // Créer un cache avec une limite de 3 éléments non épinglés
    let (_temp_dir, cache) = create_test_cache(3);

    let mut pks = Vec::new();

    // Ajouter 3 fichiers normaux (atteint la limite)
    for i in 0..3 {
        let data = format!("File {} data", i);
        let pk = add_test_file(&cache, &data).await;
        pks.push(pk);
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // Vérifier qu'on a 3 fichiers
    assert_eq!(cache.db.count_unpinned().unwrap(), 3);

    // Épingler le 2ème fichier (index 1)
    // Cela libère une place dans le comptage des non-épinglés
    let pinned_pk = pks[1].clone();
    cache.pin(&pinned_pk).await.unwrap();

    // Maintenant on a 2 fichiers non épinglés et 1 épinglé
    assert_eq!(cache.db.count_unpinned().unwrap(), 2);
    assert_eq!(cache.db.count().unwrap(), 3);

    // Ajouter 2 fichiers supplémentaires
    // Cela devrait déclencher l'éviction du plus vieux fichier non épinglé (index 0)
    for i in 3..5 {
        let data = format!("File {} data", i);
        let pk = add_test_file(&cache, &data).await;
        pks.push(pk);
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // Le cache devrait contenir :
    // - 3 fichiers non épinglés (la limite)
    // - 1 fichier épinglé
    // Total = 4 fichiers
    assert_eq!(cache.db.count_unpinned().unwrap(), 3);
    assert_eq!(cache.db.count().unwrap(), 4);

    // Vérifier que le fichier épinglé est toujours là
    assert!(cache.get(&pinned_pk).await.is_ok());
    assert!(cache.is_pinned(&pinned_pk).await.unwrap());

    // Le premier fichier (index 0, le plus vieux non épinglé) devrait avoir été évincé
    assert!(cache.get(&pks[0]).await.is_err());

    // Le 3ème fichier (index 2) devrait être présent (non épinglé mais pas le plus vieux)
    assert!(cache.get(&pks[2]).await.is_ok());

    // Les 2 derniers fichiers devraient être présents
    assert!(cache.get(&pks[3]).await.is_ok());
    assert!(cache.get(&pks[4]).await.is_ok());
}

#[tokio::test]
async fn test_pinned_count_separately() {
    let (_temp_dir, cache) = create_test_cache(5);

    // Ajouter 3 fichiers normaux
    for i in 0..3 {
        let data = format!("File {}", i);
        add_test_file(&cache, &data).await;
    }

    // Ajouter 2 fichiers épinglés
    for i in 3..5 {
        let data = format!("Pinned file {}", i);
        let pk = add_test_file(&cache, &data).await;
        cache.pin(&pk).await.unwrap();
    }

    // Le comptage total devrait être 5
    assert_eq!(cache.db.count().unwrap(), 5);

    // Le comptage non épinglé devrait être 3
    assert_eq!(cache.db.count_unpinned().unwrap(), 3);
}

#[tokio::test]
async fn test_cannot_pin_with_ttl() {
    let (_temp_dir, cache) = create_test_cache(10);

    let pk = add_test_file(&cache, "Test data").await;

    // Définir un TTL
    let expires_at = (Utc::now() + Duration::hours(1)).to_rfc3339();
    cache.set_ttl(&pk, &expires_at).await.unwrap();

    // Essayer d'épingler devrait échouer
    assert!(cache.pin(&pk).await.is_err());
}

#[tokio::test]
async fn test_cannot_set_ttl_when_pinned() {
    let (_temp_dir, cache) = create_test_cache(10);

    let pk = add_test_file(&cache, "Test data").await;

    // Épingler l'item
    cache.pin(&pk).await.unwrap();

    // Essayer de définir un TTL devrait échouer
    let expires_at = (Utc::now() + Duration::hours(1)).to_rfc3339();
    assert!(cache.set_ttl(&pk, &expires_at).await.is_err());
}

#[tokio::test]
async fn test_ttl_expiration() {
    let (_temp_dir, cache) = create_test_cache(10);

    // Ajouter un fichier avec un TTL expiré
    let pk = add_test_file(&cache, "Expiring data").await;
    let expires_at = (Utc::now() - Duration::seconds(1)).to_rfc3339(); // Déjà expiré
    cache.set_ttl(&pk, &expires_at).await.unwrap();

    // Vérifier que le fichier existe avant l'enforcement
    assert!(cache.get(&pk).await.is_ok());

    // Déclencher le nettoyage
    cache.enforce_limit().await.unwrap();

    // Le fichier devrait avoir été supprimé
    assert!(cache.get(&pk).await.is_err());
}

#[tokio::test]
async fn test_clear_ttl() {
    let (_temp_dir, cache) = create_test_cache(10);

    let pk = add_test_file(&cache, "Test data").await;

    // Définir un TTL
    let expires_at = (Utc::now() + Duration::hours(1)).to_rfc3339();
    cache.set_ttl(&pk, &expires_at).await.unwrap();

    // Vérifier que le TTL est défini
    let entry = cache.db.get(&pk, false).unwrap();
    assert!(entry.ttl_expires_at.is_some());

    // Supprimer le TTL
    cache.clear_ttl(&pk).await.unwrap();

    // Vérifier que le TTL a été supprimé
    let entry = cache.db.get(&pk, false).unwrap();
    assert!(entry.ttl_expires_at.is_none());

    // Maintenant on devrait pouvoir épingler
    assert!(cache.pin(&pk).await.is_ok());
}

#[tokio::test]
async fn test_get_expired() {
    let (_temp_dir, cache) = create_test_cache(10);

    // Ajouter un fichier non expiré
    let pk1 = add_test_file(&cache, "Non-expired data").await;
    let expires_at1 = (Utc::now() + Duration::hours(1)).to_rfc3339();
    cache.set_ttl(&pk1, &expires_at1).await.unwrap();

    // Ajouter un fichier expiré
    let pk2 = add_test_file(&cache, "Expired data").await;
    let expires_at2 = (Utc::now() - Duration::seconds(1)).to_rfc3339();
    cache.set_ttl(&pk2, &expires_at2).await.unwrap();

    // Récupérer les items expirés
    let expired = cache.db.get_expired().unwrap();

    // Seulement le deuxième fichier devrait être dans la liste
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].pk, pk2);
}

#[tokio::test]
async fn test_cache_entry_fields() {
    let (_temp_dir, cache) = create_test_cache(10);

    let pk = add_test_file(&cache, "Test data").await;

    // Vérifier les valeurs par défaut
    let entry = cache.db.get(&pk, false).unwrap();
    assert!(!entry.pinned);
    assert!(entry.ttl_expires_at.is_none());

    // Épingler et vérifier
    cache.pin(&pk).await.unwrap();
    let entry = cache.db.get(&pk, false).unwrap();
    assert!(entry.pinned);

    // Désépingler et définir un TTL
    cache.unpin(&pk).await.unwrap();
    let expires_at = (Utc::now() + Duration::hours(2)).to_rfc3339();
    cache.set_ttl(&pk, &expires_at).await.unwrap();

    let entry = cache.db.get(&pk, false).unwrap();
    assert!(!entry.pinned);
    assert!(entry.ttl_expires_at.is_some());
}
