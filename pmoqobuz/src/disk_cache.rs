#![cfg(feature = "disk-cache")]

use anyhow::anyhow;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection};
use tokio::task;

#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    pub value: T,
    pub age: Duration,
    pub fresh: bool,
}

#[async_trait::async_trait]
pub trait CacheStore: Send + Sync {
    async fn get_json<T: DeserializeOwned + Send>(
        &self,
        user_id: &str,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<Option<CacheEntry<T>>>;

    async fn put_json<T: Serialize + Send + Sync>(
        &self,
        user_id: &str,
        namespace: &str,
        key: &str,
        ttl: Duration,
        value: &T,
    ) -> anyhow::Result<()>;

    async fn invalidate(
        &self,
        user_id: &str,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<()>;

    async fn purge_expired(&self) -> anyhow::Result<usize>;
}

pub struct SqliteCacheStore {
    db_path: PathBuf,
}

impl SqliteCacheStore {
    pub fn new(db_path: PathBuf) -> anyhow::Result<Self> {
        let store = Self { db_path };
        store.init_blocking()?;
        Ok(store)
    }

    fn init_blocking(&self) -> anyhow::Result<()> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS qobuz_cache (
                user_id      TEXT NOT NULL,
                namespace    TEXT NOT NULL,
                key          TEXT NOT NULL,
                fetched_at   INTEGER NOT NULL,
                ttl_seconds  INTEGER NOT NULL,
                json         BLOB NOT NULL,
                PRIMARY KEY (user_id, namespace, key)
            );
            CREATE INDEX IF NOT EXISTS qobuz_cache_expiry
                ON qobuz_cache (fetched_at, ttl_seconds);
            "#,
        )?;
        Ok(())
    }

    fn now_seconds() -> i64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
            Err(_) => 0,
        }
    }
}

#[async_trait::async_trait]
impl CacheStore for SqliteCacheStore {
    async fn get_json<T: DeserializeOwned + Send>(
        &self,
        user_id: &str,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<Option<CacheEntry<T>>> {
        let user_id = user_id.to_owned();
        let namespace = namespace.to_owned();
        let key = key.to_owned();
        let db_path = self.db_path.clone();

        task::spawn_blocking(move || {
            let conn = Connection::open(db_path)?;
            let mut stmt = conn.prepare(
                "SELECT fetched_at, ttl_seconds, json
                 FROM qobuz_cache
                 WHERE user_id = ?1 AND namespace = ?2 AND key = ?3",
            )?;

            let result = stmt.query_row(
                params![user_id, namespace, key],
                |row| {
                    let fetched_at: i64 = row.get(0)?;
                    let ttl_seconds: i64 = row.get(1)?;
                    let data: Vec<u8> = row.get(2)?;
                    let now = Self::now_seconds();
                    let fresh = now <= fetched_at + ttl_seconds;
                    let age_secs = if now >= fetched_at {
                        (now - fetched_at) as u64
                    } else {
                        0
                    };
                    let age = Duration::from_secs(age_secs);
                    let value = serde_json::from_slice(&data)?;
                    Ok(CacheEntry {
                        value,
                        age,
                        fresh,
                    })
                },
            );

            match result {
                Ok(entry) => Ok(Some(entry)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(err.into()),
            }
        })
        .await
        .map_err(|err| anyhow!(err))?
    }

    async fn put_json<T: Serialize + Send + Sync>(
        &self,
        user_id: &str,
        namespace: &str,
        key: &str,
        ttl: Duration,
        value: &T,
    ) -> anyhow::Result<()> {
        let user_id = user_id.to_owned();
        let namespace = namespace.to_owned();
        let key = key.to_owned();
        let db_path = self.db_path.clone();
        let ttl_seconds = i64::try_from(ttl.as_secs()).unwrap_or(i64::MAX);
        let now = Self::now_seconds();
        let json = serde_json::to_vec(value)?;

        task::spawn_blocking(move || {
            let conn = Connection::open(db_path)?;
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT OR REPLACE INTO qobuz_cache
                 (user_id, namespace, key, fetched_at, ttl_seconds, json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![user_id, namespace, key, now, ttl_seconds, json],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(|err| anyhow!(err))?
    }

    async fn invalidate(
        &self,
        user_id: &str,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<()> {
        let user_id = user_id.to_owned();
        let namespace = namespace.to_owned();
        let key = key.to_owned();
        let db_path = self.db_path.clone();

        task::spawn_blocking(move || {
            let conn = Connection::open(db_path)?;
            conn.execute(
                "DELETE FROM qobuz_cache
                 WHERE user_id = ?1 AND namespace = ?2 AND key = ?3",
                params![user_id, namespace, key],
            )?;
            Ok(())
        })
        .await
        .map_err(|err| anyhow!(err))?
    }

    async fn purge_expired(&self) -> anyhow::Result<usize> {
        let db_path = self.db_path.clone();
        let now = Self::now_seconds();

        task::spawn_blocking(move || {
            let conn = Connection::open(db_path)?;
            let changes = conn.execute(
                "DELETE FROM qobuz_cache
                 WHERE (fetched_at + ttl_seconds) <= ?1",
                params![now],
            )?;
            Ok(changes)
        })
        .await
        .map_err(|err| anyhow!(err))?
    }
}

pub use {CacheEntry, CacheStore, SqliteCacheStore};
