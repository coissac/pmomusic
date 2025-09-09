package pmocover

import (
	"database/sql"
	"path/filepath"
	"time"

	_ "modernc.org/sqlite"
)

// DB représente la base SQLite du cache
type DB struct {
	conn *sql.DB
}

// InitDB ouvre ou crée la base SQLite dans le répertoire dir
func InitDB(dir string) (*DB, error) {
	path := filepath.Join(dir, "cache.db")
	conn, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, err
	}

	db := &DB{conn: conn}
	if err := db.initTables(); err != nil {
		conn.Close()
		return nil, err
	}
	return db, nil
}

// initTables crée la table covers si elle n’existe pas
func (db *DB) initTables() error {
	_, err := db.conn.Exec(`
	CREATE TABLE IF NOT EXISTS covers (
		pk TEXT PRIMARY KEY,
		source_url TEXT,
		hits INTEGER DEFAULT 0,
		last_used TEXT
	);
	`)
	return err
}

// Add ajoute ou met à jour une entrée cover
func (db *DB) Add(pk, url string) error {
	_, err := db.conn.Exec(`
	INSERT INTO covers(pk, source_url, hits, last_used)
	VALUES(?, ?, 0, ?)
	ON CONFLICT(pk) DO UPDATE SET
		source_url=excluded.source_url,
		last_used=excluded.last_used;
	`, pk, url, time.Now().UTC().Format(time.RFC3339))
	return err
}

// Get récupère une entrée
func (db *DB) Get(pk string) (*CacheEntry, error) {
	row := db.conn.QueryRow(`
	SELECT pk, source_url, hits, last_used
	FROM covers
	WHERE pk = ?
	`, pk)
	entry := &CacheEntry{}
	var lastUsed sql.NullString
	err := row.Scan(&entry.PK, &entry.SourceURL, &entry.Hits, &lastUsed)
	if err != nil {
		return nil, err
	}
	if lastUsed.Valid {
		entry.LastUsed = lastUsed.String
	}
	return entry, nil
}

// UpdateHit incrémente hits et met à jour last_used
func (db *DB) UpdateHit(pk string) error {
	_, err := db.conn.Exec(`
	UPDATE covers
	SET hits = hits + 1,
	    last_used = ?
	WHERE pk = ?
	`, time.Now().UTC().Format(time.RFC3339), pk)
	return err
}

// Purge supprime toutes les entrées de la base
func (db *DB) Purge() error {
	_, err := db.conn.Exec(`DELETE FROM covers`)
	return err
}

// GetAll récupère toutes les entrées triées par hits décroissant
func (db *DB) GetAll() ([]*CacheEntry, error) {
	rows, err := db.conn.Query(`
		SELECT pk, source_url, hits, last_used
		FROM covers
		ORDER BY hits DESC
	`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var entries []*CacheEntry
	for rows.Next() {
		entry := &CacheEntry{}
		var lastUsed sql.NullString
		if err := rows.Scan(&entry.PK, &entry.SourceURL, &entry.Hits, &lastUsed); err != nil {
			continue
		}
		if lastUsed.Valid {
			entry.LastUsed = lastUsed.String
		}
		entries = append(entries, entry)
	}
	return entries, nil
}
