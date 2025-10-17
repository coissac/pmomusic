//! Module de gestion de la base de données SQLite pour le cache
//!
//! Ce module fournit une interface générique pour gérer les métadonnées
//! des éléments en cache, avec tracking des accès et des statistiques.

use rusqlite::{Connection, params};
use serde::Serialize;
use chrono::Utc;
use std::path::Path;
use std::sync::Mutex;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Entrée de cache représentant un élément dans la base de données
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CacheEntry {
    /// Clé primaire unique de l'élément (hash SHA1 de l'URL)
    #[cfg_attr(feature = "openapi", schema(example = "1a2b3c4d5e6f7a8b"))]
    pub pk: String,
    /// URL source de l'élément
    #[cfg_attr(feature = "openapi", schema(example = "https://example.com/resource"))]
    pub source_url: String,
    /// Collection à laquelle appartient l'élément (optionnel)
    #[cfg_attr(feature = "openapi", schema(example = "album:123"))]
    pub collection: Option<String>,
    /// Nombre d'accès à l'élément
    #[cfg_attr(feature = "openapi", schema(example = 42))]
    pub hits: i32,
    /// Date/heure du dernier accès (RFC3339)
    #[cfg_attr(feature = "openapi", schema(example = "2025-01-15T10:30:00Z"))]
    pub last_used: Option<String>,
    /// Métadonnées JSON optionnelles (ex: métadonnées audio, EXIF images, etc.)
    #[cfg_attr(feature = "openapi", schema(example = r#"{"title":"Track","artist":"Artist"}"#))]
    pub metadata_json: Option<String>,
}

/// Base de données SQLite pour le cache
///
/// Gère les métadonnées des éléments en cache :
/// - Clés primaires (pk) et URLs sources
/// - Statistiques d'utilisation (hits, last_used)
/// - Opérations CRUD de base
#[derive(Debug)]
pub struct DB {
    conn: Mutex<Connection>,
    table_name: String,
}

impl DB {
    /// Initialise une nouvelle base de données avec une table personnalisée
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin vers le fichier de base de données SQLite
    /// * `table_name` - Nom de la table à créer
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmocache::db::DB;
    /// use std::path::Path;
    ///
    /// let db = DB::init(Path::new("cache.db"), "my_cache").unwrap();
    /// ```
    pub fn init(path: &Path, table_name: &str) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;

        let create_table_sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (
                pk TEXT PRIMARY KEY,
                source_url TEXT,
                collection TEXT,
                hits INTEGER DEFAULT 0,
                last_used TEXT,
                metadata_json TEXT
            )",
            table_name
        );

        conn.execute(&create_table_sql, [])?;

        // Créer un index sur la collection pour les requêtes rapides
        let create_index_sql = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_collection ON {} (collection)",
            table_name, table_name
        );

        conn.execute(&create_index_sql, [])?;

        // Créer un index composite pour optimiser la politique LRU (get_oldest)
        let create_lru_index_sql = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_lru ON {} (last_used ASC, hits ASC)",
            table_name, table_name
        );

        conn.execute(&create_lru_index_sql, [])?;

        Ok(Self {
            conn: Mutex::new(conn),
            table_name: table_name.to_string(),
        })
    }

    /// Ajoute ou met à jour une entrée dans la base de données
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'élément
    /// * `url` - URL source de l'élément
    /// * `collection` - Collection optionnelle à laquelle appartient l'élément
    pub fn add(&self, pk: &str, url: &str, collection: Option<&str>) -> rusqlite::Result<()> {
        self.add_with_metadata(pk, url, collection, None)
    }

    /// Ajoute ou met à jour une entrée avec métadonnées JSON optionnelles
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'élément
    /// * `url` - URL source de l'élément
    /// * `collection` - Collection optionnelle à laquelle appartient l'élément
    /// * `metadata_json` - Métadonnées JSON optionnelles
    pub fn add_with_metadata(
        &self,
        pk: &str,
        url: &str,
        collection: Option<&str>,
        metadata_json: Option<&str>,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "INSERT INTO {} (pk, source_url, collection, hits, last_used, metadata_json)
             VALUES (?1, ?2, ?3, 0, ?4, ?5)
             ON CONFLICT(pk) DO UPDATE SET
                 source_url = excluded.source_url,
                 collection = excluded.collection,
                 last_used = excluded.last_used,
                 metadata_json = excluded.metadata_json",
            self.table_name
        );

        conn.execute(
            &sql,
            params![pk, url, collection, Utc::now().to_rfc3339(), metadata_json],
        )?;

        Ok(())
    }

    /// Récupère une entrée de la base de données par sa clé
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'élément à récupérer
    pub fn get(&self, pk: &str) -> rusqlite::Result<CacheEntry> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT pk, source_url, collection, hits, last_used, metadata_json FROM {} WHERE pk = ?1",
            self.table_name
        );

        conn.query_row(
            &sql,
            [pk],
            |row| {
                Ok(CacheEntry {
                    pk: row.get(0)?,
                    source_url: row.get(1)?,
                    collection: row.get(2)?,
                    hits: row.get(3)?,
                    last_used: row.get(4)?,
                    metadata_json: row.get(5)?,
                })
            },
        )
    }

    /// Met à jour le compteur d'accès et la date du dernier accès
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'élément
    pub fn update_hit(&self, pk: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "UPDATE {} SET hits = hits + 1, last_used = ?1 WHERE pk = ?2",
            self.table_name
        );

        conn.execute(
            &sql,
            params![Utc::now().to_rfc3339(), pk],
        )?;

        Ok(())
    }

    /// Purge toutes les entrées de la base de données
    pub fn purge(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let sql = format!("DELETE FROM {}", self.table_name);
        conn.execute(&sql, [])?;
        Ok(())
    }

    /// Récupère toutes les entrées, triées par nombre d'accès décroissant
    pub fn get_all(&self) -> rusqlite::Result<Vec<CacheEntry>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT pk, source_url, collection, hits, last_used, metadata_json FROM {} ORDER BY hits DESC",
            self.table_name
        );

        let mut stmt = conn.prepare(&sql)?;

        let entries = stmt.query_map([], |row| {
            Ok(CacheEntry {
                pk: row.get(0)?,
                source_url: row.get(1)?,
                collection: row.get(2)?,
                hits: row.get(3)?,
                last_used: row.get(4)?,
                metadata_json: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(entries)
    }

    /// Récupère toutes les entrées d'une collection spécifique
    ///
    /// # Arguments
    ///
    /// * `collection` - Identifiant de la collection
    pub fn get_by_collection(&self, collection: &str) -> rusqlite::Result<Vec<CacheEntry>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT pk, source_url, collection, hits, last_used, metadata_json FROM {} WHERE collection = ?1 ORDER BY hits DESC",
            self.table_name
        );

        let mut stmt = conn.prepare(&sql)?;

        let entries = stmt.query_map([collection], |row| {
            Ok(CacheEntry {
                pk: row.get(0)?,
                source_url: row.get(1)?,
                collection: row.get(2)?,
                hits: row.get(3)?,
                last_used: row.get(4)?,
                metadata_json: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(entries)
    }

    /// Supprime toutes les entrées d'une collection
    ///
    /// # Arguments
    ///
    /// * `collection` - Identifiant de la collection à supprimer
    pub fn delete_collection(&self, collection: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let sql = format!("DELETE FROM {} WHERE collection = ?1", self.table_name);
        conn.execute(&sql, [collection])?;
        Ok(())
    }

    /// Supprime une entrée de la base de données
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'élément à supprimer
    pub fn delete(&self, pk: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let sql = format!("DELETE FROM {} WHERE pk = ?1", self.table_name);
        conn.execute(&sql, [pk])?;
        Ok(())
    }

    /// Compte le nombre total d'entrées dans le cache
    ///
    /// # Returns
    ///
    /// Le nombre total d'entrées
    pub fn count(&self) -> rusqlite::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let sql = format!("SELECT COUNT(*) FROM {}", self.table_name);
        let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Récupère les N entrées les plus anciennes (LRU - Least Recently Used)
    ///
    /// Trie par last_used (les plus anciens en premier), puis par hits (les moins utilisés).
    /// Utile pour implémenter une politique d'éviction LRU.
    ///
    /// # Arguments
    ///
    /// * `limit` - Nombre maximum d'entrées à récupérer
    ///
    /// # Returns
    ///
    /// Liste des entrées les plus anciennes, triées par last_used ASC
    pub fn get_oldest(&self, limit: usize) -> rusqlite::Result<Vec<CacheEntry>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT pk, source_url, collection, hits, last_used, metadata_json
             FROM {}
             ORDER BY last_used ASC, hits ASC
             LIMIT ?1",
            self.table_name
        );

        let mut stmt = conn.prepare(&sql)?;

        let entries = stmt.query_map([limit], |row| {
            Ok(CacheEntry {
                pk: row.get(0)?,
                source_url: row.get(1)?,
                collection: row.get(2)?,
                hits: row.get(3)?,
                last_used: row.get(4)?,
                metadata_json: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(entries)
    }

    /// Récupère uniquement les métadonnées JSON d'une entrée
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'élément
    ///
    /// # Returns
    ///
    /// Les métadonnées JSON si présentes, None sinon
    pub fn get_metadata_json(&self, pk: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT metadata_json FROM {} WHERE pk = ?1",
            self.table_name
        );

        conn.query_row(&sql, [pk], |row| row.get(0))
    }

    /// Met à jour uniquement les métadonnées JSON d'une entrée existante
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'élément
    /// * `metadata_json` - Métadonnées JSON à stocker
    pub fn update_metadata(&self, pk: &str, metadata_json: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "UPDATE {} SET metadata_json = ?1 WHERE pk = ?2",
            self.table_name
        );

        conn.execute(&sql, params![metadata_json, pk])?;
        Ok(())
    }
}
