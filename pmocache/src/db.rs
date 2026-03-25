//! Module de gestion de la base de données SQLite pour le cache
//!
//! Ce module fournit une interface générique pour gérer les métadonnées
//! des éléments en cache, avec tracking des accès et des statistiques.

use chrono::Utc;
use rusqlite::{params, Connection, Error, OptionalExtension};
use serde::Serialize;
use serde_json::{Map, Number, Value};
use tracing::{trace, warn};

use std::path::Path;
use std::str::FromStr;
use std::sync::{Mutex, MutexGuard};

/// Version du schéma de la base de données du cache.
///
/// Incrémenter cette constante à chaque modification incompatible du schéma.
/// Cela provoquera la suppression de la DB **et de tous les fichiers du cache**
/// au prochain démarrage.
pub const SCHEMA_VERSION: u32 = 1;
use std::time::Instant;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Entrée de cache représentant un élément dans la base de données
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CacheEntry {
    /// Clé primaire unique de l'élément (hash SHA1 de l'URL)
    #[cfg_attr(feature = "openapi", schema(example = "1a2b3c4d5e6f7a8b"))]
    pub pk: String,
    /// Lazy PK historique associé (si l'élément provient d'un téléchargement différé)
    #[cfg_attr(feature = "openapi", schema(example = "L:QOBUZ:123456"))]
    pub lazy_pk: Option<String>,
    /// URL source de l'élément
    #[cfg_attr(feature = "openapi", schema(example = "https://example.com/resource"))]
    pub id: Option<String>,
    /// Collection à laquelle appartient l'élément (optionnel)
    #[cfg_attr(feature = "openapi", schema(example = "album:123"))]
    pub collection: Option<String>,
    /// Nombre d'accès à l'élément
    #[cfg_attr(feature = "openapi", schema(example = 42))]
    pub hits: i32,
    /// Date/heure du dernier accès (RFC3339)
    #[cfg_attr(feature = "openapi", schema(example = "2025-01-15T10:30:00Z"))]
    pub last_used: Option<String>,
    /// Indique si l'élément est épinglé (ne peut pas être supprimé par LRU)
    #[cfg_attr(feature = "openapi", schema(example = false))]
    pub pinned: bool,
    /// Date/heure d'expiration du TTL (RFC3339), incompatible avec pinned=true
    #[cfg_attr(feature = "openapi", schema(example = "2025-01-20T10:30:00Z"))]
    pub ttl_expires_at: Option<String>,
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

struct ConnGuard<'a> {
    ctx: &'static str,
    guard: MutexGuard<'a, Connection>,
}

impl<'a> Drop for ConnGuard<'a> {
    fn drop(&mut self) {
        trace!("DB mutex → released ({})", self.ctx);
    }
}

impl<'a> std::ops::Deref for ConnGuard<'a> {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a> std::ops::DerefMut for ConnGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl DB {
    fn lock_conn(&self, ctx: &'static str) -> ConnGuard<'_> {
        // trace!("DB mutex → acquiring ({ctx})");
        let start = Instant::now();
        let guard = self.conn.lock().unwrap();
        let waited = start.elapsed();

        // trace!("DB mutex → acquired ({ctx}) in {:?}", waited);
        if waited > std::time::Duration::from_millis(50) {
            warn!("DB mutex wait >50 ms ({}): {:?}", ctx, waited);
        }

        ConnGuard { ctx, guard }
    }

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
    /// let db = DB::init(Path::new("cache.db")).unwrap();
    /// ```
    /// Initialise la DB. Retourne `(db, was_reset)` où `was_reset` indique si la DB
    /// a été supprimée et recréée suite à un changement de version de schéma.
    /// Dans ce cas, l'appelant doit aussi effacer les fichiers du cache.
    pub fn init(path: &Path) -> Result<(Self, bool), rusqlite::Error> {
        let was_reset = if path.exists() {
            if let Ok(conn) = Connection::open(path) {
                let version: u32 = conn
                    .query_row("PRAGMA user_version", [], |r| r.get(0))
                    .unwrap_or(0);
                if version != SCHEMA_VERSION {
                    drop(conn);
                    warn!(
                        "Cache DB schema version mismatch (found {}, expected {}), recreating",
                        version, SCHEMA_VERSION
                    );
                    std::fs::remove_file(path).ok();
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        let conn = Connection::open(path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS asset (
                pk TEXT PRIMARY KEY,
                collection TEXT,
                id TEXT,
                hits INTEGER DEFAULT 0,
                last_used TEXT,
                lazy_pk TEXT,
                pinned INTEGER DEFAULT 0 CHECK (pinned IN (0, 1)),
                ttl_expires_at TEXT
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
                FOREIGN KEY (pk) REFERENCES asset (pk) ON DELETE CASCADE ON UPDATE CASCADE
            )",
            [],
        )?;

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

        // LAZY PK SUPPORT: Index sur lazy_pk pour lookups rapides (lazy_pk → real pk)
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_asset_lazy_pk ON asset (lazy_pk)",
            [],
        )?;

        // Index unique sur lazy_pk (non-NULL) pour éviter les doublons
        // Un lazy_pk ne peut pointer que vers un seul entry
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_asset_lazy_pk_unique
             ON asset (lazy_pk) WHERE lazy_pk IS NOT NULL",
            [],
        )?;

        // Inscrire la version du schéma
        conn.execute_batch(&format!("PRAGMA user_version = {}", SCHEMA_VERSION))?;

        Ok((Self { conn: Mutex::new(conn) }, was_reset))
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
        // Bloc pour limiter la durée du lock
        {
            let conn = self.lock_conn("add_with_metadata");

            conn.execute(
                "INSERT INTO asset (pk, id, collection, hits, last_used)
                 VALUES (?1, ?2, ?3, 0, ?4)
                 ON CONFLICT(pk) DO UPDATE SET
                     id = excluded.id,
                     collection = excluded.collection,
                     last_used = excluded.last_used",
                params![pk, id, collection, Utc::now().to_rfc3339()],
            )?;
        } // Lock libéré ici

        // Appeler set_metadata après avoir libéré le lock pour éviter un deadlock
        if let Some(metadata) = metadata {
            self.set_metadata(pk, metadata)?;
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

        let mut conn = self.lock_conn("set_metadata");

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

        let conn = self.lock_conn("set_a_metadata");

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
        let conn = self.lock_conn("get_a_metadata");

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
        let conn = self.lock_conn("get_metadata");
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
    ///
    /// Cette méthode détecte automatiquement les collisions de pk :
    /// si le pk existe déjà avec une URL différente, un log d'erreur est émis.
    pub fn set_origin_url(&self, pk: &str, origin_url: &str) -> rusqlite::Result<()> {
        // Vérifier si ce pk a déjà une URL d'origine différente (détection de collision)
        if let Ok(Some(existing_url)) = self.get_origin_url(pk) {
            if existing_url != origin_url {
                tracing::error!(
                    "🚨 COLLISION DE PK DÉTECTÉE: pk='{}' existe déjà avec origin_url='{}' mais tentative d'enregistrement avec origin_url='{}'",
                    pk,
                    existing_url,
                    origin_url
                );
                tracing::error!(
                    "   Cela indique que deux fichiers différents ont généré le même pk. Considérez augmenter la taille du header pour le calcul du pk."
                );
            }
        }

        self.set_a_metadata(pk, "origin_url", Value::String(origin_url.to_owned()))
    }

    /// Récupère l'URL d'origine précédemment stockée pour un élément.
    ///
    /// Retourne `Ok(None)` lorsqu'aucune URL n'a été définie.
    pub fn get_origin_url(&self, pk: &str) -> rusqlite::Result<Option<String>> {
        match self.get_metadata_value(pk, "origin_url")? {
            Some(Value::String(url)) => Ok(Some(url)),
            Some(Value::Null) | None => Ok(None),
            Some(other) => Err(Error::InvalidParameterName(format!(
                "metadata 'origin_url' must be a string, got {other}"
            ))),
        }
    }

    /// Recherche un pk par son URL d'origine.
    ///
    /// Cette méthode permet de vérifier si un fichier avec une URL donnée
    /// est déjà en cache avant de lancer un téléchargement.
    ///
    /// # Arguments
    ///
    /// * `origin_url` - L'URL d'origine à rechercher
    ///
    /// # Returns
    ///
    /// * `Ok(Some(pk))` - Le pk du fichier en cache avec cette URL
    /// * `Ok(None)` - Aucun fichier avec cette URL n'est en cache
    pub fn get_pk_by_origin_url(&self, origin_url: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.lock_conn("get_pk_by_origin_url");

        conn.query_row(
            "SELECT pk FROM metadata WHERE key = 'origin_url' AND value = ?",
            [origin_url],
            |row| row.get(0),
        )
        .optional()
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
        let conn = self.lock_conn("get_metadata_value");

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
            let conn = self.lock_conn("get");
            conn.query_row(
                "SELECT pk, lazy_pk, id, collection, hits, last_used, pinned, ttl_expires_at \
                 FROM asset \
                 WHERE pk = ?1",
                [pk],
                |row| {
                    Ok(CacheEntry {
                        pk: row.get(0)?,
                        lazy_pk: row.get::<_, Option<String>>(1)?,
                        id: row.get::<_, Option<String>>(2)?,
                        collection: row.get(3)?,
                        hits: row.get(4)?,
                        last_used: row.get(5)?,
                        pinned: row.get::<_, i32>(6)? != 0,
                        ttl_expires_at: row.get::<_, Option<String>>(7)?,
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
            let conn = self.lock_conn("get_from_id");
            conn.query_row(
                "SELECT pk, lazy_pk, id, collection, hits, last_used, pinned, ttl_expires_at \
             FROM asset \
             WHERE collection = ?1 AND id = ?2",
                params![collection, id],
                |row| {
                    Ok(CacheEntry {
                        pk: row.get(0)?,
                        lazy_pk: row.get::<_, Option<String>>(1)?,
                        id: row.get::<_, Option<String>>(2)?,
                        collection: row.get(3)?,
                        hits: row.get(4)?,
                        last_used: row.get(5)?,
                        pinned: row.get::<_, i32>(6)? != 0,
                        ttl_expires_at: row.get::<_, Option<String>>(7)?,
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

    /// Indique si une collection contient un identifiant donné.
    ///
    /// Retourne `false` si l'enregistrement n'existe pas ou si la requête échoue.
    pub fn does_collection_contain_id(&self, collection: &str, id: &str) -> bool {
        let conn = self.lock_conn("does_collection_contain_id");
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM asset WHERE collection = ?1 AND id = ?2)",
            params![collection, id],
            |row| row.get::<_, i64>(0),
        )
        .map(|flag| flag != 0)
        .unwrap_or(false)
    }

    /// Retourne la clé primaire associée à la paire `(collection, id)`.
    ///
    /// # Errors
    ///
    /// Retourne `QueryReturnedNoRows` si aucun enregistrement ne correspond.
    pub fn get_pk_from_id(&self, collection: &str, id: &str) -> rusqlite::Result<String> {
        let conn = self.lock_conn("get_pk_from_id");
        conn.query_row(
            "SELECT pk FROM asset WHERE collection = ?1 AND id = ?2",
            params![collection, id],
            |row| row.get(0),
        )
    }

    /// Définit ou remplace l'identifiant logique (`id`) d'une entrée.
    ///
    /// Retourne `QueryReturnedNoRows` si la clé primaire est inconnue.
    pub fn set_id(&self, pk: &str, id: &str) -> rusqlite::Result<()> {
        let conn = self.lock_conn("set_id");
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
        let conn = self.lock_conn("update_hit");

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
        let conn = self.lock_conn("purge");
        conn.execute("DELETE FROM asset", [])?;
        Ok(())
    }

    /// Récupère toutes les entrées, triées par nombre d'accès décroissant.
    ///
    /// # Arguments
    ///
    /// * `include_metadata` - Ajoute les métadonnées à chaque entrée si `true`.
    pub fn get_all(&self, include_metadata: bool) -> rusqlite::Result<Vec<CacheEntry>> {
        let mut entries = {
            let conn = self.lock_conn("get_all");

            let mut stmt = conn.prepare(
                "SELECT pk, lazy_pk, id, collection, hits, last_used, pinned, ttl_expires_at
                 FROM asset
                 ORDER BY hits DESC",
            )?;

            let rows = stmt.query_map([], |row| {
                Ok(CacheEntry {
                    pk: row.get(0)?,
                    lazy_pk: row.get::<_, Option<String>>(1)?,
                    id: row.get::<_, Option<String>>(2)?,
                    collection: row.get(3)?,
                    hits: row.get(4)?,
                    last_used: row.get(5)?,
                    pinned: row.get::<_, i32>(6)? != 0,
                    ttl_expires_at: row.get::<_, Option<String>>(7)?,
                    metadata: None,
                })
            })?;

            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

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
        let mut entries = {
            let conn = self.lock_conn("get_by_collection");

            let mut stmt = conn.prepare(
                "SELECT pk, lazy_pk, id, collection, hits, last_used, pinned, ttl_expires_at
                  FROM asset
                  WHERE collection = ?1 ORDER BY hits DESC",
            )?;
            let rows = stmt.query_map([collection], |row| {
                Ok(CacheEntry {
                    pk: row.get(0)?,
                    lazy_pk: row.get::<_, Option<String>>(1)?,
                    id: row.get::<_, Option<String>>(2)?,
                    collection: row.get(3)?,
                    hits: row.get(4)?,
                    last_used: row.get(5)?,
                    pinned: row.get::<_, i32>(6)? != 0,
                    ttl_expires_at: row.get::<_, Option<String>>(7)?,
                    metadata: None,
                })
            })?;

            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

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
        let conn = self.lock_conn("delete_collection");
        conn.execute("DELETE FROM asset WHERE collection = ?1", [collection])?;
        Ok(())
    }

    /// Supprime une entrée de la base de données ainsi que ses métadonnées.
    pub fn delete(&self, pk: &str) -> rusqlite::Result<()> {
        let conn = self.lock_conn("delete");
        conn.execute("DELETE FROM asset WHERE pk = ?1", [pk])?;
        Ok(())
    }

    /// Compte le nombre total d'entrées dans le cache
    ///
    /// # Returns
    ///
    /// Le nombre total d'entrées
    pub fn count(&self) -> rusqlite::Result<usize> {
        let conn = self.lock_conn("count");
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM asset", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Compte le nombre d'entrées non épinglées dans le cache
    ///
    /// Les items épinglés ne comptent pas dans la limite du cache.
    ///
    /// # Returns
    ///
    /// Le nombre d'entrées non épinglées
    pub fn count_unpinned(&self) -> rusqlite::Result<usize> {
        let conn = self.lock_conn("count_unpinned");
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM asset WHERE pinned = 0", [], |row| {
                row.get(0)
            })?;
        Ok(count as usize)
    }

    /// Épingle un item pour le protéger de l'éviction LRU
    ///
    /// Un item épinglé ne peut pas être supprimé automatiquement par la politique LRU
    /// et ne compte pas dans la limite du cache.
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'item à épingler
    ///
    /// # Errors
    ///
    /// Retourne une erreur si l'item a un TTL défini (incompatibilité métier)
    pub fn pin(&self, pk: &str) -> rusqlite::Result<()> {
        let conn = self.lock_conn("pin");

        // Vérifier que l'item n'a pas de TTL
        let has_ttl: bool = conn
            .query_row(
                "SELECT ttl_expires_at IS NOT NULL FROM asset WHERE pk = ?1",
                [pk],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(false);

        if has_ttl {
            return Err(Error::InvalidParameterName(
                "Cannot pin an item with TTL set".to_owned(),
            ));
        }

        let updated = conn.execute("UPDATE asset SET pinned = 1 WHERE pk = ?1", [pk])?;

        if updated == 0 {
            return Err(Error::QueryReturnedNoRows);
        }

        Ok(())
    }

    /// Désépingle un item pour le rendre à nouveau éligible à l'éviction LRU
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'item à désépingler
    pub fn unpin(&self, pk: &str) -> rusqlite::Result<()> {
        let conn = self.lock_conn("unpin");
        let updated = conn.execute("UPDATE asset SET pinned = 0 WHERE pk = ?1", [pk])?;

        if updated == 0 {
            return Err(Error::QueryReturnedNoRows);
        }

        Ok(())
    }

    /// Vérifie si un item est épinglé
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'item
    ///
    /// # Returns
    ///
    /// `true` si l'item est épinglé, `false` sinon
    pub fn is_pinned(&self, pk: &str) -> rusqlite::Result<bool> {
        let conn = self.lock_conn("is_pinned");
        let pinned: i32 =
            conn.query_row("SELECT pinned FROM asset WHERE pk = ?1", [pk], |row| {
                row.get(0)
            })?;
        Ok(pinned != 0)
    }

    /// Définit le TTL (Time To Live) d'un item
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'item
    /// * `expires_at` - Date/heure d'expiration au format RFC3339
    ///
    /// # Errors
    ///
    /// Retourne une erreur si l'item est épinglé (incompatibilité métier)
    pub fn set_ttl(&self, pk: &str, expires_at: &str) -> rusqlite::Result<()> {
        let conn = self.lock_conn("set_ttl");

        // Vérifier que l'item n'est pas épinglé
        let is_pinned: bool = conn
            .query_row("SELECT pinned != 0 FROM asset WHERE pk = ?1", [pk], |row| {
                row.get(0)
            })
            .optional()?
            .unwrap_or(false);

        if is_pinned {
            return Err(Error::InvalidParameterName(
                "Cannot set TTL on a pinned item".to_owned(),
            ));
        }

        let updated = conn.execute(
            "UPDATE asset SET ttl_expires_at = ?2 WHERE pk = ?1",
            params![pk, expires_at],
        )?;

        if updated == 0 {
            return Err(Error::QueryReturnedNoRows);
        }

        Ok(())
    }

    /// Supprime le TTL d'un item
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de l'item
    pub fn clear_ttl(&self, pk: &str) -> rusqlite::Result<()> {
        let conn = self.lock_conn("clear_ttl");
        let updated = conn.execute("UPDATE asset SET ttl_expires_at = NULL WHERE pk = ?1", [pk])?;

        if updated == 0 {
            return Err(Error::QueryReturnedNoRows);
        }

        Ok(())
    }

    /// Récupère les items expirés (TTL dépassé)
    ///
    /// # Returns
    ///
    /// Liste des entrées dont le TTL est dépassé
    pub fn get_expired(&self) -> rusqlite::Result<Vec<CacheEntry>> {
        let conn = self.lock_conn("get_expired");
        let now = Utc::now().to_rfc3339();

        let mut stmt = conn.prepare(
            "SELECT pk, lazy_pk, id, collection, hits, last_used, pinned, ttl_expires_at
             FROM asset
             WHERE ttl_expires_at IS NOT NULL AND ttl_expires_at < ?1",
        )?;

        let entries = stmt
            .query_map([now], |row| {
                Ok(CacheEntry {
                    pk: row.get(0)?,
                    lazy_pk: row.get::<_, Option<String>>(1)?,
                    id: row.get::<_, Option<String>>(2)?,
                    collection: row.get(3)?,
                    hits: row.get(4)?,
                    last_used: row.get(5)?,
                    pinned: row.get::<_, i32>(6)? != 0,
                    ttl_expires_at: row.get::<_, Option<String>>(7)?,
                    metadata: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(entries)
    }

    /// Récupère les N entrées les plus anciennes (LRU - Least Recently Used)
    ///
    /// Trie par last_used (les plus anciens en premier), puis par hits (les moins utilisés).
    /// Utile pour implémenter une politique d'éviction LRU.
    /// Les items épinglés sont EXCLUS de cette liste (ils ne peuvent pas être évincés).
    ///
    /// # Arguments
    ///
    /// * `limit` - Nombre maximum d'entrées à récupérer
    ///
    /// # Returns
    ///
    /// Liste des entrées les plus anciennes (non épinglées), triées par last_used ASC
    pub fn get_oldest(&self, limit: usize) -> rusqlite::Result<Vec<CacheEntry>> {
        let conn = self.lock_conn("get_oldest");

        let mut stmt = conn.prepare(
            "SELECT pk, lazy_pk, id, collection, hits, last_used, pinned, ttl_expires_at
             FROM asset
             WHERE pinned = 0
             ORDER BY last_used ASC, hits ASC
             LIMIT ?1",
        )?;

        let entries = stmt
            .query_map([limit], |row| {
                Ok(CacheEntry {
                    pk: row.get(0)?,
                    lazy_pk: row.get::<_, Option<String>>(1)?,
                    id: row.get::<_, Option<String>>(2)?,
                    collection: row.get(3)?,
                    hits: row.get(4)?,
                    last_used: row.get(5)?,
                    pinned: row.get::<_, i32>(6)? != 0,
                    ttl_expires_at: row.get::<_, Option<String>>(7)?,
                    metadata: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(entries)
    }

    // ============================================================================
    // LAZY PK SUPPORT
    // ============================================================================

    /// Ajoute une entrée en mode lazy (pk = lazy_pk tant que non téléchargé)
    ///
    /// Utilisé pour créer des entries sans télécharger le fichier.
    /// Le lazy_pk est calculé à partir de l'URL et sert temporairement
    /// également de pk pour satisfaire les contraintes de clé étrangère.
    ///
    /// # Arguments
    ///
    /// * `lazy_pk` - PK temporaire (format "L:" + hash(url))
    /// * `id` - Identifiant optionnel
    /// * `collection` - Collection optionnelle
    pub fn add_lazy(
        &self,
        lazy_pk: &str,
        id: Option<&str>,
        collection: Option<&str>,
    ) -> rusqlite::Result<()> {
        let conn = self.lock_conn("add_lazy");

        conn.execute(
            "INSERT INTO asset (pk, lazy_pk, id, collection, hits, last_used)
             VALUES (?1, ?1, ?2, ?3, 0, ?4)",
            params![lazy_pk, id, collection, Utc::now().to_rfc3339()],
        )?;

        Ok(())
    }

    /// Récupère le real pk associé à un lazy_pk
    ///
    /// # Arguments
    ///
    /// * `lazy_pk` - Le lazy PK à rechercher
    ///
    /// # Returns
    ///
    /// * `Ok(Some(pk))` - Le real pk si le fichier a été téléchargé
    /// * `Ok(None)` - Pas encore téléchargé ou lazy_pk inconnu
    pub fn get_pk_by_lazy_pk(&self, lazy_pk: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.lock_conn("get_pk_by_lazy_pk");

        let result: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT pk, lazy_pk FROM asset WHERE lazy_pk = ?1",
                [lazy_pk],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        Ok(result.and_then(|(pk, lazy)| {
            if let Some(lazy_pk_value) = lazy {
                if pk == lazy_pk_value {
                    None
                } else {
                    Some(pk)
                }
            } else {
                Some(pk)
            }
        }))
    }

    /// Vérifie l'existence d'une entrée lazy sans télécharger le fichier
    ///
    /// Retourne `true` si une ligne avec `lazy_pk` existe, même si `pk` est `NULL`.
    pub fn has_lazy_entry(&self, lazy_pk: &str) -> rusqlite::Result<bool> {
        let conn = self.lock_conn("has_lazy_entry");

        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM asset WHERE lazy_pk = ?1 LIMIT 1",
                [lazy_pk],
                |row| row.get(0),
            )
            .optional()?;

        Ok(exists.is_some())
    }

    /// Transition d'une entry lazy vers downloaded (ajoute le real pk)
    ///
    /// Cette méthode est appelée après le téléchargement d'un fichier lazy.
    /// Elle crée une nouvelle entry avec le real pk ET garde le lazy_pk
    /// pour permettre aux Control Points de continuer à utiliser l'URL lazy.
    ///
    /// # Arguments
    ///
    /// * `lazy_pk` - Le lazy PK de l'entry originale
    /// * `real_pk` - Le real PK calculé après téléchargement
    pub fn update_lazy_to_downloaded(&self, lazy_pk: &str, real_pk: &str) -> rusqlite::Result<()> {
        let mut conn = self.lock_conn("update_lazy_to_downloaded");
        let tx = conn.transaction()?;

        let (current_pk, collection, id, hits): (String, Option<String>, Option<String>, i32) = tx
            .query_row(
                "SELECT pk, collection, id, hits FROM asset WHERE lazy_pk = ?1",
                [lazy_pk],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| Error::QueryReturnedNoRows)?;

        if current_pk == real_pk {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        let hits_to_add = if hits > 0 { hits } else { 1 };

        // Supprimer d'éventuelles métadonnées résiduelles associées au futur pk réel
        // (peut arriver si un ancien téléchargement a laissé des traces sans asset correspondant).
        tx.execute("DELETE FROM metadata WHERE pk = ?1", [real_pk])?;

        let updated = tx.execute(
            "UPDATE asset
             SET pk = ?1,
                 lazy_pk = ?2,
                 collection = COALESCE(?3, collection),
                 id = COALESCE(?4, id),
                 hits = hits + ?5,
                 last_used = ?6
             WHERE lazy_pk = ?7",
            params![real_pk, lazy_pk, collection, id, hits_to_add, now, lazy_pk],
        )?;

        if updated == 0 {
            return Err(Error::QueryReturnedNoRows);
        }

        // Compter les métadonnées encore sous l'ancien lazy_pk avant le commit
        let meta_under_lazy: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM metadata WHERE pk = ?1",
                [lazy_pk],
                |r| r.get(0),
            )
            .unwrap_or(0);

        tx.commit()?;

        tracing::debug!(
            "update_lazy_to_downloaded: {} → {} ({} metadata rows still under lazy_pk)",
            lazy_pk, real_pk, meta_under_lazy
        );

        Ok(())
    }

    /// Recherche une entry par son origin_url
    ///
    /// Retourne (pk, lazy_pk) si trouvé. Vérifie à la fois les entries
    /// eager (avec pk) et lazy (avec lazy_pk).
    ///
    /// # Arguments
    ///
    /// * `url` - L'URL d'origine à rechercher
    ///
    /// # Returns
    ///
    /// * `Ok(Some((Some(pk), Some(lazy_pk))))` - Entry téléchargée (lazy→eager)
    /// * `Ok(Some((Some(pk), None)))` - Entry eager (jamais lazy)
    /// * `Ok(Some((None, Some(lazy_pk))))` - Entry lazy (pas encore téléchargée)
    /// * `Ok(None)` - URL inconnue
    pub fn get_entry_by_url(
        &self,
        url: &str,
    ) -> rusqlite::Result<Option<(Option<String>, Option<String>)>> {
        let conn = self.lock_conn("get_entry_by_url");

        // Chercher via origin_url dans metadata
        // On joint avec asset pour récupérer pk et lazy_pk
        let raw: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT a.pk, a.lazy_pk
                 FROM asset a
                 JOIN metadata m ON a.pk = m.pk
                 WHERE m.key = 'origin_url' AND m.value = ?1
                 LIMIT 1",
                [url],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let result = raw.map(|(pk, lazy_pk)| {
            if let Some(ref lazy) = lazy_pk {
                if pk == *lazy {
                    (None, Some(lazy.clone()))
                } else {
                    (Some(pk), lazy_pk)
                }
            } else {
                (Some(pk), lazy_pk)
            }
        });

        Ok(result)
    }

    /// Retourne une entry à partir d'un pk ou lazy_pk.
    pub fn get_entry_by_pk_or_lazy_pk(
        &self,
        value: &str,
    ) -> rusqlite::Result<Option<(Option<String>, Option<String>, Option<String>)>> {
        let conn = self.lock_conn("get_entry_by_pk_or_lazy_pk");
        conn.query_row(
            "SELECT pk, lazy_pk, collection FROM asset WHERE pk = ?1 OR lazy_pk = ?1 LIMIT 1",
            [value],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
    }

    /// Met à jour le compteur d'accès pour une entry lazy (pk = NULL)
    ///
    /// # Arguments
    ///
    /// * `lazy_pk` - Le lazy PK de l'entry
    pub fn update_hit_by_lazy_pk(&self, lazy_pk: &str) -> rusqlite::Result<()> {
        let conn = self.lock_conn("update_hit_by_lazy_pk");

        conn.execute(
            "UPDATE asset
             SET hits = hits + 1, last_used = ?1
             WHERE lazy_pk = ?2",
            params![Utc::now().to_rfc3339(), lazy_pk],
        )?;

        Ok(())
    }

    /// Enregistre l'URL d'origine pour une entry lazy
    ///
    /// Contrairement à `set_origin_url()` qui utilise le pk, cette méthode
    /// utilise le lazy_pk comme clé dans la table metadata (car pk = NULL).
    ///
    /// # Arguments
    ///
    /// * `lazy_pk` - Le lazy PK de l'entry
    /// * `origin_url` - L'URL d'origine à stocker
    pub fn set_origin_url_for_lazy(&self, lazy_pk: &str, origin_url: &str) -> rusqlite::Result<()> {
        self.set_a_metadata_by_key(lazy_pk, "origin_url", Value::String(origin_url.to_owned()))
    }

    /// Version générique de set_a_metadata qui accepte une clé arbitraire
    ///
    /// Utilisé en interne pour stocker des métadonnées avec lazy_pk au lieu de pk
    pub fn set_a_metadata_by_key(
        &self,
        key: &str,
        metadata_key: &str,
        value: Value,
    ) -> rusqlite::Result<()> {
        let (value_type, value_text): (&str, Option<String>) = match value {
            Value::Null => ("null", None),
            Value::Bool(b) => ("boolean", Some(b.to_string())),
            Value::Number(n) => ("number", Some(n.to_string())),
            Value::String(s) => ("string", Some(s)),
            Value::Array(arr) => ("string", Some(Value::Array(arr).to_string())),
            Value::Object(map) => ("string", Some(Value::Object(map).to_string())),
        };

        let conn = self.lock_conn("set_a_metadata_by_key");

        conn.execute(
            "INSERT INTO metadata (pk, key, value_type, value)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(pk, key) DO UPDATE SET
                 value_type = excluded.value_type,
                 value = excluded.value",
            params![key, metadata_key, value_type, value_text.as_deref()],
        )?;

        Ok(())
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
