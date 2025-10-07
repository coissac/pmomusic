use rusqlite::{Connection, params};
use serde::Serialize;
use chrono::Utc;
use std::path::Path;
use std::sync::Mutex;

#[cfg(feature = "pmoserver")]
use utoipa::ToSchema;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "pmoserver", derive(ToSchema))]
pub struct CacheEntry {
    /// Clé primaire unique de l'image (hash SHA1 de l'URL)
    #[cfg_attr(feature = "pmoserver", schema(example = "1a2b3c4d5e6f7a8b"))]
    pub pk: String,
    /// URL source de l'image
    #[cfg_attr(feature = "pmoserver", schema(example = "https://example.com/cover.jpg"))]
    pub source_url: String,
    /// Nombre d'accès à l'image
    #[cfg_attr(feature = "pmoserver", schema(example = 42))]
    pub hits: i32,
    /// Date/heure du dernier accès (RFC3339)
    #[cfg_attr(feature = "pmoserver", schema(example = "2025-01-15T10:30:00Z"))]
    pub last_used: Option<String>,
}

#[derive(Debug)]
pub struct DB {
    conn: Mutex<Connection>,
}

impl DB {
    pub fn init(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS covers (
                pk TEXT PRIMARY KEY,
                source_url TEXT,
                hits INTEGER DEFAULT 0,
                last_used TEXT
            )",
            [],
        )?;

        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn add(&self, pk: &str, url: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO covers (pk, source_url, hits, last_used)
             VALUES (?1, ?2, 0, ?3)
             ON CONFLICT(pk) DO UPDATE SET
                 source_url = excluded.source_url,
                 last_used = excluded.last_used",
            params![pk, url, Utc::now().to_rfc3339()],
        )?;

        Ok(())
    }

    pub fn get(&self, pk: &str) -> rusqlite::Result<CacheEntry> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT pk, source_url, hits, last_used FROM covers WHERE pk = ?1",
            [pk],
            |row| {
                Ok(CacheEntry {
                    pk: row.get(0)?,
                    source_url: row.get(1)?,
                    hits: row.get(2)?,
                    last_used: row.get(3)?,
                })
            },
        )
    }

    pub fn update_hit(&self, pk: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE covers SET hits = hits + 1, last_used = ?1 WHERE pk = ?2",
            params![Utc::now().to_rfc3339(), pk],
        )?;

        Ok(())
    }

    pub fn purge(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM covers", [])?;
        Ok(())
    }

    pub fn get_all(&self) -> rusqlite::Result<Vec<CacheEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pk, source_url, hits, last_used FROM covers ORDER BY hits DESC",
        )?;

        let entries = stmt.query_map([], |row| {
            Ok(CacheEntry {
                pk: row.get(0)?,
                source_url: row.get(1)?,
                hits: row.get(2)?,
                last_used: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(entries)
    }

    pub fn delete(&self, pk: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM covers WHERE pk = ?1", [pk])?;
        Ok(())
    }
}
