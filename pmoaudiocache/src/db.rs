//! Module de base de données étendu pour le cache audio
//!
//! Ce module étend la DB générique de pmocache avec des champs
//! spécifiques aux métadonnées audio pour permettre le service
//! immédiat des informations avant la fin de la conversion.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

use crate::metadata::AudioMetadata;

#[cfg(feature = "pmoserver")]
use utoipa::ToSchema;

/// Entrée de cache audio avec métadonnées complètes
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "pmoserver", derive(ToSchema))]
pub struct AudioCacheEntry {
    /// Clé primaire unique (hash SHA1 de l'URL)
    pub pk: String,
    /// URL source
    pub source_url: String,
    /// Collection (artiste:album)
    pub collection: Option<String>,
    /// Nombre d'accès
    pub hits: i32,
    /// Dernière utilisation
    pub last_used: Option<String>,
    /// Métadonnées audio (stockées en JSON)
    pub metadata: AudioMetadata,
    /// État de conversion (pending, converting, completed, failed)
    pub conversion_status: String,
}

/// Base de données SQLite pour le cache audio
///
/// Étend la DB générique avec :
/// - Métadonnées audio complètes en JSON
/// - État de conversion pour le traitement asynchrone
#[derive(Debug)]
pub struct AudioDB {
    conn: Mutex<Connection>,
}

impl AudioDB {
    /// Initialise une nouvelle base de données audio
    pub fn init(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS audio_tracks (
                pk TEXT PRIMARY KEY,
                source_url TEXT,
                collection TEXT,
                hits INTEGER DEFAULT 0,
                last_used TEXT,
                metadata_json TEXT,
                conversion_status TEXT DEFAULT 'pending'
            )",
            [],
        )?;

        // Index sur la collection
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_audio_tracks_collection
             ON audio_tracks (collection)",
            [],
        )?;

        // Index sur le statut de conversion
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_audio_tracks_conversion
             ON audio_tracks (conversion_status)",
            [],
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Ajoute une entrée avec métadonnées
    pub fn add(&self, pk: &str, url: &str, collection: Option<&str>, metadata: &AudioMetadata) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let metadata_json = serde_json::to_string(metadata)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

        conn.execute(
            "INSERT INTO audio_tracks (pk, source_url, collection, hits, last_used, metadata_json, conversion_status)
             VALUES (?1, ?2, ?3, 0, ?4, ?5, 'pending')
             ON CONFLICT(pk) DO UPDATE SET
                 source_url = excluded.source_url,
                 collection = excluded.collection,
                 metadata_json = excluded.metadata_json,
                 last_used = excluded.last_used",
            params![pk, url, collection, chrono::Utc::now().to_rfc3339(), metadata_json],
        )?;

        Ok(())
    }

    /// Récupère une entrée avec métadonnées
    pub fn get(&self, pk: &str) -> rusqlite::Result<AudioCacheEntry> {
        let conn = self.conn.lock().unwrap();

        conn.query_row(
            "SELECT pk, source_url, collection, hits, last_used, metadata_json, conversion_status
             FROM audio_tracks WHERE pk = ?1",
            [pk],
            |row| {
                let metadata_json: String = row.get(5)?;
                let metadata: AudioMetadata = serde_json::from_str(&metadata_json)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(e)
                    ))?;

                Ok(AudioCacheEntry {
                    pk: row.get(0)?,
                    source_url: row.get(1)?,
                    collection: row.get(2)?,
                    hits: row.get(3)?,
                    last_used: row.get(4)?,
                    metadata,
                    conversion_status: row.get(6)?,
                })
            },
        )
    }

    /// Met à jour le statut de conversion
    pub fn update_conversion_status(&self, pk: &str, status: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE audio_tracks SET conversion_status = ?1 WHERE pk = ?2",
            params![status, pk],
        )?;
        Ok(())
    }

    /// Met à jour le compteur d'accès
    pub fn update_hit(&self, pk: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE audio_tracks SET hits = hits + 1, last_used = ?1 WHERE pk = ?2",
            params![chrono::Utc::now().to_rfc3339(), pk],
        )?;
        Ok(())
    }

    /// Récupère toutes les entrées d'une collection
    pub fn get_by_collection(&self, collection: &str) -> rusqlite::Result<Vec<AudioCacheEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pk, source_url, collection, hits, last_used, metadata_json, conversion_status
             FROM audio_tracks WHERE collection = ?1 ORDER BY hits DESC",
        )?;

        let entries = stmt.query_map([collection], |row| {
            let metadata_json: String = row.get(5)?;
            let metadata: AudioMetadata = serde_json::from_str(&metadata_json)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(e)
                ))?;

            Ok(AudioCacheEntry {
                pk: row.get(0)?,
                source_url: row.get(1)?,
                collection: row.get(2)?,
                hits: row.get(3)?,
                last_used: row.get(4)?,
                metadata,
                conversion_status: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(entries)
    }

    /// Récupère toutes les entrées
    pub fn get_all(&self) -> rusqlite::Result<Vec<AudioCacheEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pk, source_url, collection, hits, last_used, metadata_json, conversion_status
             FROM audio_tracks ORDER BY hits DESC",
        )?;

        let entries = stmt.query_map([], |row| {
            let metadata_json: String = row.get(5)?;
            let metadata: AudioMetadata = serde_json::from_str(&metadata_json)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(e)
                ))?;

            Ok(AudioCacheEntry {
                pk: row.get(0)?,
                source_url: row.get(1)?,
                collection: row.get(2)?,
                hits: row.get(3)?,
                last_used: row.get(4)?,
                metadata,
                conversion_status: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(entries)
    }

    /// Supprime une entrée
    pub fn delete(&self, pk: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM audio_tracks WHERE pk = ?1", [pk])?;
        Ok(())
    }

    /// Purge toutes les entrées
    pub fn purge(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM audio_tracks", [])?;
        Ok(())
    }
}
