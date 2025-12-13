#![cfg(feature = "disk-cache")]

use pmoqobuz::disk_cache::{CacheStore, SqliteCacheStore};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn sqlite_cache_returns_fresh_entries() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("cache.sqlite");
    let store = SqliteCacheStore::new(path)?;

    let data = vec!["album".to_string()];
    store
        .put_json(
            "user",
            "favorites_albums",
            "all",
            Duration::from_secs(3600),
            &data,
        )
        .await?;

    let entry = store
        .get_json::<Vec<String>>("user", "favorites_albums", "all")
        .await?
        .expect("cache entry");

    assert!(entry.fresh);
    assert_eq!(entry.value, data);

    Ok(())
}

#[tokio::test]
async fn sqlite_cache_marks_entries_as_stale_after_ttl() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("cache.sqlite");
    let store = SqliteCacheStore::new(path)?;

    let data = vec!["track".to_string()];
    store
        .put_json(
            "user",
            "favorites_tracks",
            "all",
            Duration::from_secs(1),
            &data,
        )
        .await?;

    sleep(Duration::from_secs(2)).await;

    let entry = store
        .get_json::<Vec<String>>("user", "favorites_tracks", "all")
        .await?
        .expect("cache entry");

    assert!(!entry.fresh);

    Ok(())
}

#[tokio::test]
async fn sqlite_cache_purge_expired_removes_entries() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("cache.sqlite");
    let store = SqliteCacheStore::new(path)?;

    let data = vec!["playlist".to_string()];
    store
        .put_json(
            "user",
            "user_playlists",
            "all",
            Duration::from_secs(1),
            &data,
        )
        .await?;

    sleep(Duration::from_secs(2)).await;

    let removed = store.purge_expired().await?;
    assert_eq!(removed, 1);

    let entry = store
        .get_json::<Vec<String>>("user", "user_playlists", "all")
        .await?;

    assert!(entry.is_none());

    Ok(())
}
