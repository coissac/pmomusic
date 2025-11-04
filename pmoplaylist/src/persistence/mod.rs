//! Gestion de la persistance SQLite pour les playlists

use crate::playlist::core::PlaylistConfig;
use crate::playlist::record::Record;
use crate::Result;
use rusqlite::{params, Connection};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Gestionnaire de persistance (une base pour toutes les playlists)
pub struct PersistenceManager {
    conn: Arc<Mutex<Connection>>,
}

impl PersistenceManager {
    /// Initialise le gestionnaire de persistance
    pub fn new(db_path: &Path) -> Result<Self> {
        // Créer le répertoire parent si nécessaire
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                crate::Error::PersistenceError(format!("Failed to create directory: {}", e))
            })?;
        }

        let conn = Connection::open(db_path).map_err(|e| {
            crate::Error::PersistenceError(format!("Failed to open database: {}", e))
        })?;

        // Créer les tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS playlists (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                max_size INTEGER,
                default_ttl_secs INTEGER,
                created_at INTEGER NOT NULL,
                last_modified INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| {
            crate::Error::PersistenceError(format!("Failed to create playlists table: {}", e))
        })?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS tracks (
                playlist_id TEXT NOT NULL,
                added_at INTEGER NOT NULL PRIMARY KEY,
                cache_pk TEXT NOT NULL,
                ttl_secs INTEGER,
                FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE
            )",
            [],
        )
        .map_err(|e| {
            crate::Error::PersistenceError(format!("Failed to create tracks table: {}", e))
        })?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tracks_playlist ON tracks(playlist_id, added_at)",
            [],
        )
        .map_err(|e| crate::Error::PersistenceError(format!("Failed to create index: {}", e)))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tracks_cache_pk ON tracks(cache_pk)",
            [],
        )
        .map_err(|e| crate::Error::PersistenceError(format!("Failed to create index: {}", e)))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Sauvegarde une playlist complète
    pub async fn save_playlist(
        &self,
        id: &str,
        title: &str,
        config: &PlaylistConfig,
        tracks: &VecDeque<Arc<Record>>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

        // Upsert playlist metadata
        conn.execute(
            "INSERT OR REPLACE INTO playlists (id, title, max_size, default_ttl_secs, created_at, last_modified)
             VALUES (?1, ?2, ?3, ?4, 
                     COALESCE((SELECT created_at FROM playlists WHERE id = ?1), ?5), 
                     ?5)",
            params![
                id,
                title,
                config.max_size.map(|s| s as i64),
                config.default_ttl.map(|d| d.as_secs() as i64),
                now_nanos,
            ],
        ).map_err(|e| crate::Error::PersistenceError(format!("Failed to save playlist: {}", e)))?;

        // Supprimer les anciens tracks
        conn.execute("DELETE FROM tracks WHERE playlist_id = ?1", params![id])
            .map_err(|e| {
                crate::Error::PersistenceError(format!("Failed to delete old tracks: {}", e))
            })?;

        // Insérer les nouveaux tracks
        for record in tracks {
            conn.execute(
                "INSERT INTO tracks (playlist_id, added_at, cache_pk, ttl_secs)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    id,
                    record.added_at_nanos(),
                    &record.cache_pk,
                    record.ttl.map(|d| d.as_secs() as i64),
                ],
            )
            .map_err(|e| {
                crate::Error::PersistenceError(format!("Failed to insert track: {}", e))
            })?;
        }

        Ok(())
    }

    /// Charge une playlist
    pub async fn load_playlist(
        &self,
        id: &str,
    ) -> Result<Option<(String, PlaylistConfig, VecDeque<Arc<Record>>)>> {
        let conn = self.conn.lock().unwrap();

        // Charger les métadonnées
        let mut stmt = conn
            .prepare("SELECT title, max_size, default_ttl_secs FROM playlists WHERE id = ?1")
            .map_err(|e| {
                crate::Error::PersistenceError(format!("Failed to prepare statement: {}", e))
            })?;

        let result = stmt.query_row(params![id], |row| {
            let title: String = row.get(0)?;
            let max_size: Option<i64> = row.get(1)?;
            let default_ttl_secs: Option<i64> = row.get(2)?;

            Ok((
                title,
                PlaylistConfig {
                    max_size: max_size.map(|s| s as usize),
                    default_ttl: default_ttl_secs.map(|s| Duration::from_secs(s as u64)),
                },
            ))
        });

        let (title, config) = match result {
            Ok(data) => data,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => {
                return Err(crate::Error::PersistenceError(format!(
                    "Failed to load playlist: {}",
                    e
                )))
            }
        };

        // Charger les tracks
        let mut stmt = conn.prepare(
            "SELECT added_at, cache_pk, ttl_secs FROM tracks WHERE playlist_id = ?1 ORDER BY added_at ASC"
        ).map_err(|e| crate::Error::PersistenceError(format!("Failed to prepare statement: {}", e)))?;

        let rows = stmt
            .query_map(params![id], |row| {
                let added_at_nanos: i64 = row.get(0)?;
                let cache_pk: String = row.get(1)?;
                let ttl_secs: Option<i64> = row.get(2)?;

                let added_at = UNIX_EPOCH + Duration::from_nanos(added_at_nanos as u64);
                let ttl = ttl_secs.map(|s| Duration::from_secs(s as u64));

                Ok(Record {
                    cache_pk,
                    added_at,
                    ttl,
                })
            })
            .map_err(|e| {
                crate::Error::PersistenceError(format!("Failed to query tracks: {}", e))
            })?;

        let mut tracks = VecDeque::new();
        for row in rows {
            let record = row.map_err(|e| {
                crate::Error::PersistenceError(format!("Failed to read track: {}", e))
            })?;
            tracks.push_back(Arc::new(record));
        }

        Ok(Some((title, config, tracks)))
    }

    /// Supprime une playlist
    pub async fn delete_playlist(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])
            .map_err(|e| {
                crate::Error::PersistenceError(format!("Failed to delete playlist: {}", e))
            })?;
        Ok(())
    }

    /// Liste toutes les playlists persistantes
    pub async fn list_playlist_ids(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id FROM playlists").map_err(|e| {
            crate::Error::PersistenceError(format!("Failed to prepare statement: {}", e))
        })?;

        let rows = stmt.query_map([], |row| row.get(0)).map_err(|e| {
            crate::Error::PersistenceError(format!("Failed to query playlists: {}", e))
        })?;

        let mut ids = Vec::new();
        for row in rows {
            ids.push(row.map_err(|e| {
                crate::Error::PersistenceError(format!("Failed to read id: {}", e))
            })?);
        }

        Ok(ids)
    }

    /// Supprime tous les tracks contenant un cache_pk donné
    pub async fn remove_by_cache_pk(&self, cache_pk: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM tracks WHERE cache_pk = ?1", params![cache_pk])
            .map_err(|e| {
                crate::Error::PersistenceError(format!("Failed to remove tracks: {}", e))
            })?;
        Ok(())
    }
}
