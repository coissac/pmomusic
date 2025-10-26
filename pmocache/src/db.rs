//! Module de gestion de la base de données SQLite pour le cache
//!
//! Ce module fournit une interface générique pour gérer les métadonnées
//! des éléments en cache, avec tracking des accès et des statistiques.

use chrono::Utc;
use rusqlite::{params, Connection, Error, OptionalExtension};
use serde::Serialize;
use serde_json::{Map, Number, Value};

use std::path::Path;
use std::str::FromStr;
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
    pub id: String,
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
    #[cfg_attr(
        feature = "openapi",
        schema(example = r#"{"title":"Track","artist":"Artist"}"#)
    )]
    pub metadata: Option<Value>,
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
    pub fn init(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS asset (
                pk TEXT PRIMARY KEY,
                collection TEXT,
                id TEXT,
                hits INTEGER DEFAULT 0,
                last_used TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS metadata (
                pk TEXT,
                key TEXT,
                value_type    TEXT    NOT NULL CHECK (value_type IN ('string','number','boolean','null')),
                value TEXT,
                PRIMARY KEY (pk, key),
                FOREIGN KEY (pk) REFERENCES asset (pk) ON DELETE CASCADE
            )"
            , [])?;

        // Créer un index sur la collection pour les requêtes rapides
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_asset_collection 
                             ON ASSET (collection)",
            [],
        )?;

        // Créer un index composite pour optimiser la politique LRU (get_oldest)
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_asset_lru 
                             ON asset (last_used ASC, hits ASC)",
            [],
        )?;

        // Crée un index composite pour rendre unique les ids si défini dans une collection
        conn.execute(
            "CREATE UNIQUE INDEX 
                             IF NOT EXISTS asset_collection_id_unique
                             ON asset (collection, id)
                             WHERE id IS NOT NULL;",
            [],
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Ajoute ou met à jour une entrée dans la base de données
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'élément
    /// * `url` - URL source de l'élément
    /// * `collection` - Collection optionnelle à laquelle appartient l'élément
    pub fn add(
        &self,
        pk: &str,
        id: Option<&str>,
        collection: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.add_with_metadata(pk, id, collection, None)
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
        id: Option<&str>,
        collection: Option<&str>,
        metadata: Option<&Value>,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO asset (pk, id, collection, hits, last_used)
             VALUES (?1, ?2, ?3, 0, ?4)
             ON CONFLICT(pk) DO UPDATE SET
                 id = excluded.id,
                 collection = excluded.collection,
                 last_used = excluded.last_used",
            params![pk, id, collection, Utc::now().to_rfc3339()],
        )?;

        if metadata.is_some() {
            self.set_metadata(pk, metadata.unwrap())?
        }

        Ok(())
    }

    /// Remplace toutes les métadonnées associées à une entrée.
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'élément ciblé.
    /// * `metadata` - Objet JSON complet décrivant les nouvelles métadonnées.
    ///
    /// # Errors
    ///
    /// Retourne une erreur si `metadata` n'est pas un objet JSON ou si l'écriture
    /// SQLite échoue.
    pub fn set_metadata(&self, pk: &str, metadata: &Value) -> rusqlite::Result<()> {
        let metadata_obj = metadata.as_object().ok_or_else(|| {
            Error::InvalidParameterName("metadata must be a JSON object".to_owned())
        })?;

        let mut conn = self.conn.lock().unwrap();

        let tx = conn.transaction()?;

        tx.execute("DELETE FROM metadata WHERE pk = ?1", params![pk])?;

        for (key, value) in metadata_obj.iter() {
            let (value_type, value_text): (&str, Option<String>) = match value {
                Value::Null => ("null", None),
                Value::Bool(b) => ("boolean", Some(b.to_string())),
                Value::Number(n) => ("number", Some(n.to_string())),
                Value::String(s) => ("string", Some(s.clone())),
                Value::Array(_) | Value::Object(_) => ("string", Some(value.to_string())),
            };

            tx.execute(
                "INSERT INTO metadata (pk, key, value_type, value) VALUES (?1, ?2, ?3, ?4)",
                params![pk, key, value_type, value_text.as_deref()],
            )?;
        }

        tx.commit()
    }

    /// Insère ou met à jour une métadonnée individuelle.
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'élément concerné.
    /// * `key` - Nom de la métadonnée à enregistrer.
    /// * `value` - Valeur JSON à stocker pour cette clé.
    pub fn set_a_metadata(&self, pk: &str, key: &str, value: Value) -> rusqlite::Result<()> {
        let (value_type, value_text): (&str, Option<String>) = match value {
            Value::Null => ("null", None),
            Value::Bool(b) => ("boolean", Some(b.to_string())),
            Value::Number(n) => ("number", Some(n.to_string())),
            Value::String(s) => ("string", Some(s)),
            Value::Array(arr) => ("string", Some(Value::Array(arr).to_string())),
            Value::Object(map) => ("string", Some(Value::Object(map).to_string())),
        };

        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO metadata (pk, key, value_type, value)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(pk, key) DO UPDATE SET
             value_type = excluded.value_type,
             value = excluded.value",
            params![pk, key, value_type, value_text.as_deref()],
        )?;

        Ok(())
    }

    /// Alias interne pour récupérer une métadonnée individuelle.
    ///
    /// Préférer `get_metadata_value` pour les appels externes.
    pub fn get_a_metadata(&self, pk: &str, key: &str) -> rusqlite::Result<Option<Value>> {
        let conn = self.conn.lock().unwrap();

        conn.query_row(
            "SELECT value_type, value FROM metadata WHERE pk = ?1 AND key = ?2",
            params![pk, key],
            |row| {
                let value_type: String = row.get(0)?;
                let raw: Option<String> = row.get(1)?;
                decode_metadata_value(key, &value_type, raw)
            },
        )
        .optional()
    }

    /// Récupère toutes les métadonnées d'une entrée sous forme d'objet JSON.
    ///
    /// Retourne `Ok(None)` si aucune métadonnée n'est présente.
    pub fn get_metadata(&self, pk: &str) -> rusqlite::Result<Option<Value>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, value_type, value FROM metadata WHERE pk = ?1")?;

        let rows = stmt.query_map([pk], |row| {
            let key: String = row.get(0)?;
            let value_type: String = row.get(1)?;
            let value: Option<String> = row.get(2)?;
            Ok((key, value_type, value))
        })?;

        let mut metadata = Map::new();
        let mut found = false;

        for row in rows {
            let (key, value_type, raw) = row?;
            found = true;

            let value = match value_type.as_str() {
                "null" => Value::Null,
                "boolean" => {
                    let raw = raw.as_deref().ok_or_else(|| {
                        Error::InvalidParameterName(format!(
                            "missing boolean metadata for key '{key}'"
                        ))
                    })?;
                    let parsed = raw.parse::<bool>().map_err(|_| {
                        Error::InvalidParameterName(format!(
                            "invalid boolean metadata for key '{key}'"
                        ))
                    })?;
                    Value::Bool(parsed)
                }
                "number" => {
                    let raw = raw.as_deref().ok_or_else(|| {
                        Error::InvalidParameterName(format!(
                            "missing number metadata for key '{key}'"
                        ))
                    })?;
                    let number = Number::from_str(raw).map_err(|_| {
                        Error::InvalidParameterName(format!(
                            "invalid number metadata for key '{key}'"
                        ))
                    })?;
                    Value::Number(number)
                }
                "string" => Value::String(raw.unwrap_or_default()),
                other => {
                    return Err(Error::InvalidParameterName(format!(
                        "unknown metadata type '{other}' for key '{key}'"
                    )))
                }
            };

            metadata.insert(key, value);
        }

        if found {
            Ok(Some(Value::Object(metadata)))
        } else {
            Ok(None)
        }
    }

    /// Enregistre l'URL d'origine liée à un élément du cache.
    pub fn set_origin_url(&self, pk: &str, origin_url: &str) -> rusqlite::Result<()> {
        self.set_a_metadata(pk, "origin_url", Value::String(origin_url.to_owned()))
    }

    /// Récupère l'URL d'origine précédemment stockée pour un élément.
    ///
    /// Retourne `Ok(None)` si aucune URL n'est définie.
    pub fn get_origin_url(&self, pk: &str) -> rusqlite::Result<Option<String>> {
        match self.get_metadata_value(pk, "origin_url")? {
            Some(Value::String(url)) => Ok(Some(url)),
            Some(Value::Null) | None => Ok(None),
            Some(other) => Err(Error::InvalidParameterName(format!(
                "metadata 'origin_url' must be a string, got {other}"
            ))),
        }
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
        Ok(self.get_metadata(pk)?.map(|value| value.to_string()))
    }

    /// Récupère une métadonnée individuelle, si elle existe.
    pub fn get_metadata_value(&self, pk: &str, key: &str) -> rusqlite::Result<Option<Value>> {
        let conn = self.conn.lock().unwrap();

        conn.query_row(
            "SELECT value_type, value FROM metadata WHERE pk = ?1 AND key = ?2",
            params![pk, key],
            |row| {
                let value_type: String = row.get(0)?;
                let raw: Option<String> = row.get(1)?;
                decode_metadata_value(key, &value_type, raw)
            },
        )
        .optional()
    }

    /// Récupère une entrée de la base de données par sa clé
    ///
    /// # Arguments
    /// * `pk` - Clé primaire de l'élément à récupérer.
    /// * `with_metadata` - Charge les métadonnées associées si `true`.
    pub fn get(&self, pk: &str, with_metadata: bool) -> rusqlite::Result<CacheEntry> {
        let mut entry = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT pk, id, collection, hits, last_used \
                 FROM asset \
                 WHERE pk = ?1",
                [pk],
                |row| {
                    Ok(CacheEntry {
                        pk: row.get(0)?,
                        id: row.get(1)?,
                        collection: row.get(2)?,
                        hits: row.get(3)?,
                        last_used: row.get(4)?,
                        metadata: None,
                    })
                },
            )?
        };

        if with_metadata {
            entry.metadata = self.get_metadata(&entry.pk)?;
        }

        Ok(entry)
    }

    /// Récupère une entrée en utilisant la paire `(collection, id)`.
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection dans laquelle chercher.
    /// * `id` - Identifiant logique de l'élément.
    /// * `with_metadata` - Charge les métadonnées associées si `true`.
    pub fn get_from_id(
        &self,
        collection: &str,
        id: &str,
        with_metadata: bool,
    ) -> rusqlite::Result<CacheEntry> {
        let mut entry = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT pk, id, collection, hits, last_used \
             FROM asset \
             WHERE collection = ?1 AND id = ?2",
                params![collection, id],
                |row| {
                    Ok(CacheEntry {
                        pk: row.get(0)?,
                        id: row.get(1)?,
                        collection: row.get(2)?,
                        hits: row.get(3)?,
                        last_used: row.get(4)?,
                        metadata: None,
                    })
                },
            )?
        };

        if with_metadata {
            entry.metadata = self.get_metadata(&entry.pk)?;
        }

        Ok(entry)
    }

    /// Définit ou remplace l'identifiant logique (`id`) d'une entrée.
    ///
    /// Retourne `QueryReturnedNoRows` si la clé primaire est inconnue.
    pub fn set_id(&self, pk: &str, id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute("UPDATE asset SET id = ?2 WHERE pk = ?1", params![pk, id])?;

        if updated == 0 {
            return Err(Error::QueryReturnedNoRows);
        }

        Ok(())
    }

    /// Met à jour le compteur d'accès et la date du dernier accès
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'élément
    pub fn update_hit(&self, pk: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            &"UPDATE asset 
                            SET hits = hits + 1, last_used = ?1 
                            WHERE pk = ?2",
            params![Utc::now().to_rfc3339(), pk],
        )?;

        Ok(())
    }

    /// Purge toutes les entrées de la base de données.
    pub fn purge(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM asset", [])?;
        Ok(())
    }

    /// Récupère toutes les entrées, triées par nombre d'accès décroissant.
    ///
    /// # Arguments
    ///
    /// * `include_metadata` - Ajoute les métadonnées à chaque entrée si `true`.
    pub fn get_all(&self, include_metadata: bool) -> rusqlite::Result<Vec<CacheEntry>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT pk, id, collection, hits, last_used 
                 FROM asset 
                 ORDER BY hits DESC",
        )?;

        let mut entries = stmt
            .query_map([], |row| {
                Ok(CacheEntry {
                    pk: row.get(0)?,
                    id: row.get(1)?,
                    collection: row.get(2)?,
                    hits: row.get(3)?,
                    last_used: row.get(4)?,
                    metadata: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        if include_metadata {
            for entry in entries.iter_mut() {
                entry.metadata = self.get_metadata(&entry.pk)?;
            }
        }

        Ok(entries)
    }

    /// Récupère toutes les entrées d'une collection spécifique
    ///
    /// # Arguments
    ///
    /// * `collection` - Identifiant de la collection.
    /// * `include_metadata` - Ajoute les métadonnées à chaque entrée si `true`.
    pub fn get_by_collection(
        &self,
        collection: &str,
        include_metadata: bool,
    ) -> rusqlite::Result<Vec<CacheEntry>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT pk, id, collection, hits, last_used 
                  FROM asset 
                  WHERE collection = ?1 ORDER BY hits DESC",
        )?;

        let mut entries = stmt
            .query_map([collection], |row| {
                Ok(CacheEntry {
                    pk: row.get(0)?,
                    id: row.get(1)?,
                    collection: row.get(2)?,
                    hits: row.get(3)?,
                    last_used: row.get(4)?,
                    metadata: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        if include_metadata {
            for entry in entries.iter_mut() {
                entry.metadata = self.get_metadata(&entry.pk)?;
            }
        }

        Ok(entries)
    }

    /// Supprime toutes les entrées d'une collection.
    ///
    /// Les métadonnées associées sont supprimées automatiquement grâce à la
    /// contrainte `ON DELETE CASCADE`.
    pub fn delete_collection(&self, collection: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM asset WHERE collection = ?1", [collection])?;
        Ok(())
    }

    /// Supprime une entrée de la base de données ainsi que ses métadonnées.
    pub fn delete(&self, pk: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM asset WHERE pk = ?1", [pk])?;
        Ok(())
    }

    /// Compte le nombre total d'entrées dans le cache
    ///
    /// # Returns
    ///
    /// Le nombre total d'entrées
    pub fn count(&self) -> rusqlite::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM asset", [], |row| row.get(0))?;
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

        let mut stmt = conn.prepare(
            "SELECT pk, source_url, collection, hits, last_used, metadata_json
             FROM asset
             ORDER BY last_used ASC, hits ASC
             LIMIT ?1",
        )?;

        let entries = stmt
            .query_map([limit], |row| {
                Ok(CacheEntry {
                    pk: row.get(0)?,
                    id: row.get(1)?,
                    collection: row.get(2)?,
                    hits: row.get(3)?,
                    last_used: row.get(4)?,
                    metadata: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(entries)
    }
}

/// Convertit une ligne de la table `metadata` en valeur JSON.
fn decode_metadata_value(
    key: &str,
    value_type: &str,
    raw: Option<String>,
) -> rusqlite::Result<Value> {
    match value_type {
        "null" => Ok(Value::Null),
        "boolean" => {
            let raw = raw.as_deref().ok_or_else(|| {
                Error::InvalidParameterName(format!("missing boolean metadata for '{key}'"))
            })?;
            raw.parse::<bool>().map(Value::Bool).map_err(|_| {
                Error::InvalidParameterName(format!("invalid boolean metadata for '{key}'"))
            })
        }
        "number" => {
            let raw = raw.as_deref().ok_or_else(|| {
                Error::InvalidParameterName(format!("missing number metadata for '{key}'"))
            })?;
            Number::from_str(raw).map(Value::Number).map_err(|_| {
                Error::InvalidParameterName(format!("invalid number metadata for '{key}'"))
            })
        }
        "string" => {
            let raw = raw.unwrap_or_default();
            let trimmed = raw.trim_start();
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                if let Ok(json) = serde_json::from_str::<Value>(&raw) {
                    return Ok(json);
                }
            }
            Ok(Value::String(raw))
        }
        other => Err(Error::InvalidParameterName(format!(
            "unknown metadata type '{other}' for key '{key}'"
        ))),
    }
}
