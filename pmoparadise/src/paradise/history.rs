//! History persistence for Radio Paradise playback.
//!
//! The worker pushes every completed track into the history backend while
//! keeping the latest entries available for UPnP browsing.  We use SQLite
//! for persistent storage with an abstract trait for testability.

use super::config::HistoryConfig;
use crate::models::Song;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;
use tokio::task::spawn_blocking;

/// Serializable record describing a played track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub track_id: String,
    pub channel_id: u8,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub duration_ms: u64,
    pub song: SongSnapshot,
}

/// Minimal snapshot of a Radio Paradise song at playback time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongSnapshot {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub cover_url: Option<String>,
}

impl SongSnapshot {
    pub fn title(&self) -> &str {
        &self.title
    }
}

impl From<&Song> for SongSnapshot {
    fn from(song: &Song) -> Self {
        Self {
            title: song.title.clone(),
            artist: song.artist.clone(),
            album: song.album.clone(),
            cover_url: song.cover.clone(),
        }
    }
}

/// Abstract persistence interface.
#[async_trait]
pub trait HistoryBackend: Send + Sync {
    async fn append(&self, entry: HistoryEntry) -> anyhow::Result<()>;
    async fn recent(&self, limit: usize) -> anyhow::Result<Vec<HistoryEntry>>;
    async fn len(&self) -> anyhow::Result<usize>;
    async fn truncate(&self, keep: usize) -> anyhow::Result<()>;
}

/// Creates a history backend from configuration.
///
/// This always creates a SQLite-based backend using the configured database path.
pub fn history_backend_from_config(
    config: &HistoryConfig,
) -> anyhow::Result<Arc<dyn HistoryBackend>> {
    let backend = SqliteHistoryBackend::new(&config.database_path)?;
    Ok(Arc::new(backend))
}


#[async_trait]
impl HistoryBackend for MemoryHistoryBackend {
    async fn append(&self, entry: HistoryEntry) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().await;
        entries.push(entry);
        Ok(())
    }

    async fn recent(&self, limit: usize) -> anyhow::Result<Vec<HistoryEntry>> {
        let entries = self.entries.lock().await;
        let total = entries.len();
        let start = total.saturating_sub(limit);
        Ok(entries[start..].to_vec())
    }

    async fn len(&self) -> anyhow::Result<usize> {
        Ok(self.entries.lock().await.len())
    }

    async fn truncate(&self, keep: usize) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().await;
        if entries.len() > keep {
            let drop_count = entries.len() - keep;
            entries.drain(0..drop_count);
        }
        Ok(())
    }
}

pub struct SqliteHistoryBackend {
    conn: Arc<StdMutex<rusqlite::Connection>>,
}

impl SqliteHistoryBackend {
    pub fn new(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = rusqlite::Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", &"WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS paradise_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                track_id TEXT NOT NULL,
                channel_id INTEGER NOT NULL,
                started_at_ms INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                title TEXT,
                artist TEXT,
                album TEXT,
                cover_url TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_history_started_at ON paradise_history(started_at_ms);",
        )?;

        Ok(Self {
            conn: Arc::new(StdMutex::new(conn)),
        })
    }

    fn conn(&self) -> Arc<StdMutex<rusqlite::Connection>> {
        self.conn.clone()
    }
}

#[async_trait]
impl HistoryBackend for SqliteHistoryBackend {
    async fn append(&self, entry: HistoryEntry) -> anyhow::Result<()> {
        let conn = self.conn();
        spawn_blocking(move || -> anyhow::Result<()> {
            let conn = conn.lock().unwrap();
            conn.execute(
                "INSERT INTO paradise_history (track_id, channel_id, started_at_ms, duration_ms, title, artist, album, cover_url)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    entry.track_id,
                    entry.channel_id as i64,
                    entry.started_at.timestamp_millis(),
                    entry.duration_ms as i64,
                    entry.song.title,
                    entry.song.artist,
                    entry.song.album,
                    entry.song.cover_url,
                ],
            )?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    async fn recent(&self, limit: usize) -> anyhow::Result<Vec<HistoryEntry>> {
        let conn = self.conn();
        let limit = limit as i64;
        spawn_blocking(move || -> anyhow::Result<Vec<HistoryEntry>> {
            let conn = conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT track_id, channel_id, started_at_ms, duration_ms, title, artist, album, cover_url
                 FROM paradise_history
                 ORDER BY started_at_ms DESC
                 LIMIT ?1",
            )?;

            let mut rows = stmt.query([limit])?;
            let mut entries = Vec::new();
            while let Some(row) = rows.next()? {
                let started_at_ms: i64 = row.get(2)?;
                let started_at = DateTime::<Utc>::from_timestamp_millis(started_at_ms)
                    .ok_or_else(|| anyhow::anyhow!("Invalid timestamp in history"))?;
                let entry = HistoryEntry {
                    track_id: row.get(0)?,
                    channel_id: row.get::<_, i64>(1)? as u8,
                    started_at,
                    duration_ms: row.get::<_, i64>(3)? as u64,
                    song: SongSnapshot {
                        title: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        artist: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        album: row.get(6)?,
                        cover_url: row.get(7)?,
                    },
                };
                entries.push(entry);
            }
            Ok(entries)
        })
        .await?
    }

    async fn len(&self) -> anyhow::Result<usize> {
        let conn = self.conn();
        let count = spawn_blocking(move || -> anyhow::Result<usize> {
            let conn = conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT COUNT(*) FROM paradise_history")?;
            let count: i64 = stmt.query_row([], |row| row.get(0))?;
            Ok(count as usize)
        })
        .await??;
        Ok(count)
    }

    async fn truncate(&self, keep: usize) -> anyhow::Result<()> {
        let conn = self.conn();
        spawn_blocking(move || -> anyhow::Result<()> {
            let conn = conn.lock().unwrap();
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM paradise_history", [], |row| {
                    row.get(0)
                })?;
            let keep = keep as i64;
            if count <= keep {
                return Ok(());
            }
            let to_remove = count - keep;
            conn.execute(
                "DELETE FROM paradise_history
                 WHERE id IN (
                     SELECT id FROM paradise_history
                     ORDER BY started_at_ms ASC
                     LIMIT ?1
                 )",
                rusqlite::params![to_remove],
            )?;
            Ok(())
        })
        .await??;
        Ok(())
    }
}
