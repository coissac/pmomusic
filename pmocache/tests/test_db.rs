use pmocache::db::DB;
use serde_json::{json, Value};
use tempfile::TempDir;

/// Crée une DB temporaire pour les tests
fn create_test_db() -> (TempDir, DB) {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = DB::init(&db_path).unwrap();
    (temp_dir, db)
}

#[test]
fn test_db_init() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = DB::init(&db_path);
    assert!(db.is_ok());
    assert!(db_path.exists());
}

#[test]
fn test_add_and_get() {
    let (_temp_dir, db) = create_test_db();

    let pk = "test_pk_123";
    let id = Some("test_id");
    let collection = Some("test_collection");

    // Ajouter une entrée
    let result = db.add(pk, id, collection);
    assert!(result.is_ok());

    // Récupérer l'entrée
    let entry = db.get(pk, false);
    assert!(entry.is_ok());

    let entry = entry.unwrap();
    assert_eq!(entry.pk, pk);
    assert_eq!(entry.id.as_deref(), id);
    assert_eq!(entry.collection.as_deref(), collection);
    assert_eq!(entry.hits, 0);
}

#[test]
fn test_add_with_metadata() {
    let (_temp_dir, db) = create_test_db();

    let pk = "test_pk_456";
    let metadata = json!({
        "title": "Test Track",
        "artist": "Test Artist",
        "duration": 180,
        "bitrate": 320
    });

    // Ajouter avec métadonnées
    let result = db.add_with_metadata(pk, None, None, Some(&metadata));
    assert!(result.is_ok());

    // Récupérer l'entrée avec métadonnées
    let entry = db.get(pk, true).unwrap();
    assert_eq!(entry.pk, pk);
    assert!(entry.metadata.is_some());

    let stored_metadata = entry.metadata.unwrap();
    assert_eq!(stored_metadata["title"], "Test Track");
    assert_eq!(stored_metadata["artist"], "Test Artist");
    assert_eq!(stored_metadata["duration"], 180);
    assert_eq!(stored_metadata["bitrate"], 320);
}

#[test]
fn test_update_hit() {
    let (_temp_dir, db) = create_test_db();

    let pk = "test_pk_789";
    db.add(pk, None, None).unwrap();

    // Récupérer l'entrée initiale
    let entry = db.get(pk, false).unwrap();
    let initial_hits = entry.hits;
    let initial_last_used = entry.last_used.clone();

    // Attendre un peu pour que le timestamp change
    std::thread::sleep(std::time::Duration::from_millis(10));

    // Mettre à jour le hit
    db.update_hit(pk).unwrap();

    // Vérifier que hits a augmenté et last_used a changé
    let entry = db.get(pk, false).unwrap();
    assert_eq!(entry.hits, initial_hits + 1);
    assert_ne!(entry.last_used, initial_last_used);
}

#[test]
fn test_delete() {
    let (_temp_dir, db) = create_test_db();

    let pk = "test_pk_delete";
    db.add(pk, None, None).unwrap();

    // Vérifier que l'entrée existe
    assert!(db.get(pk, false).is_ok());

    // Supprimer l'entrée
    let result = db.delete(pk);
    assert!(result.is_ok());

    // Vérifier que l'entrée n'existe plus
    assert!(db.get(pk, false).is_err());
}

#[test]
fn test_get_by_collection() {
    let (_temp_dir, db) = create_test_db();

    let collection = "test_collection";

    // Ajouter plusieurs entrées dans la même collection
    db.add("pk1", None, Some(collection)).unwrap();
    db.add("pk2", None, Some(collection)).unwrap();
    db.add("pk3", None, Some("other_collection")).unwrap();

    // Récupérer les entrées de la collection
    let entries = db.get_by_collection(collection, false).unwrap();

    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.pk == "pk1"));
    assert!(entries.iter().any(|e| e.pk == "pk2"));
    assert!(!entries.iter().any(|e| e.pk == "pk3"));
}

#[test]
fn test_delete_collection() {
    let (_temp_dir, db) = create_test_db();

    let collection = "test_collection_to_delete";

    db.add("pk1", None, Some(collection)).unwrap();
    db.add("pk2", None, Some(collection)).unwrap();
    db.add("pk3", None, Some("other_collection")).unwrap();

    // Supprimer la collection
    let result = db.delete_collection(collection);
    assert!(result.is_ok());

    // Vérifier que les entrées de la collection sont supprimées
    let entries = db.get_by_collection(collection, false).unwrap();
    assert_eq!(entries.len(), 0);

    // Vérifier que l'autre collection existe toujours
    assert!(db.get("pk3", false).is_ok());
}

#[test]
fn test_get_oldest() {
    let (_temp_dir, db) = create_test_db();

    // Ajouter plusieurs entrées avec des timestamps différents
    db.add("pk1", None, None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));

    db.add("pk2", None, None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));

    db.add("pk3", None, None).unwrap();

    // Mettre à jour le hit de pk1 pour le rendre plus récent
    std::thread::sleep(std::time::Duration::from_millis(10));
    db.update_hit("pk1").unwrap();

    // Récupérer les 2 plus anciennes entrées
    let oldest = db.get_oldest(2).unwrap();

    assert_eq!(oldest.len(), 2);
    // pk2 et pk3 devraient être les plus anciennes
    assert!(oldest.iter().any(|e| e.pk == "pk2"));
    assert!(oldest.iter().any(|e| e.pk == "pk3"));
}

#[test]
fn test_count() {
    let (_temp_dir, db) = create_test_db();

    assert_eq!(db.count().unwrap(), 0);

    db.add("pk1", None, None).unwrap();
    assert_eq!(db.count().unwrap(), 1);

    db.add("pk2", None, None).unwrap();
    assert_eq!(db.count().unwrap(), 2);

    db.delete("pk1").unwrap();
    assert_eq!(db.count().unwrap(), 1);
}

#[test]
fn test_purge() {
    let (_temp_dir, db) = create_test_db();

    db.add("pk1", None, None).unwrap();
    db.add("pk2", None, None).unwrap();
    db.add("pk3", None, None).unwrap();

    assert_eq!(db.count().unwrap(), 3);

    // Purger toutes les entrées
    let result = db.purge();
    assert!(result.is_ok());

    assert_eq!(db.count().unwrap(), 0);
}

#[test]
fn test_origin_url() {
    let (_temp_dir, db) = create_test_db();

    let pk = "test_pk_url";
    let url = "https://example.com/test.flac";

    db.add(pk, None, None).unwrap();
    db.set_origin_url(pk, url).unwrap();

    let retrieved_url = db.get_origin_url(pk).unwrap();
    assert_eq!(retrieved_url, Some(url.to_string()));
}

#[test]
fn test_get_from_id() {
    let (_temp_dir, db) = create_test_db();

    let pk = "test_pk_by_id";
    let collection = "my_collection";
    let id = "my_unique_id";

    db.add(pk, Some(id), Some(collection)).unwrap();

    // Récupérer par (collection, id)
    let entry = db.get_from_id(collection, id, false).unwrap();
    assert_eq!(entry.pk, pk);
    assert_eq!(entry.id.as_deref(), Some(id));
    assert_eq!(entry.collection.as_deref(), Some(collection));
}

#[test]
fn test_does_collection_contain_id() {
    let (_temp_dir, db) = create_test_db();

    let collection = "my_collection";
    let id = "my_id";

    assert!(!db.does_collection_contain_id(collection, id));

    db.add("pk", Some(id), Some(collection)).unwrap();

    assert!(db.does_collection_contain_id(collection, id));
}

#[test]
fn test_get_pk_from_id() {
    let (_temp_dir, db) = create_test_db();

    let pk = "test_pk_123";
    let collection = "my_collection";
    let id = "my_id";

    db.add(pk, Some(id), Some(collection)).unwrap();

    let retrieved_pk = db.get_pk_from_id(collection, id).unwrap();
    assert_eq!(retrieved_pk, pk);
}

#[test]
fn test_set_id() {
    let (_temp_dir, db) = create_test_db();

    let pk = "test_pk";
    db.add(pk, None, None).unwrap();

    // Définir l'id
    let new_id = "new_id";
    db.set_id(pk, new_id).unwrap();

    let entry = db.get(pk, false).unwrap();
    assert_eq!(entry.id.as_deref(), Some(new_id));
}

#[test]
fn test_metadata_types() {
    let (_temp_dir, db) = create_test_db();

    let pk = "test_pk_types";
    db.add(pk, None, None).unwrap();

    // Tester les différents types de métadonnées
    db.set_a_metadata(pk, "string_val", Value::String("test".to_string())).unwrap();
    db.set_a_metadata(pk, "number_val", json!(42)).unwrap();
    db.set_a_metadata(pk, "bool_val", Value::Bool(true)).unwrap();
    db.set_a_metadata(pk, "null_val", Value::Null).unwrap();

    // Vérifier les valeurs
    assert_eq!(db.get_metadata_value(pk, "string_val").unwrap(), Some(Value::String("test".to_string())));
    assert_eq!(db.get_metadata_value(pk, "number_val").unwrap(), Some(json!(42)));
    assert_eq!(db.get_metadata_value(pk, "bool_val").unwrap(), Some(Value::Bool(true)));
    assert_eq!(db.get_metadata_value(pk, "null_val").unwrap(), Some(Value::Null));
}
