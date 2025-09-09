package pmocover

import (
	"bytes"
	"crypto/sha1"
	"encoding/hex"
	"errors"
	"image"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"sync"
)

// CacheEntry représente une image stockée (original + dérivés)
type CacheEntry struct {
	PK        string `json:"pk"`
	SourceURL string `json:"source_url"`
	Hits      int    `json:"hits"`
	LastUsed  string `json:"last_used"` // ISO8601
}

// Cache persistant avec SQLite
type Cache struct {
	dir   string
	limit int
	db    *DB

	mu sync.Mutex
}

// NewCache ouvre ou crée un cache persistant avec SQLite
func NewCache(dir string, limit int) (*Cache, error) {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return nil, err
	}

	db, err := InitDB(dir)
	if err != nil {
		return nil, err
	}

	return &Cache{
		dir:   dir,
		limit: limit,
		db:    db,
	}, nil
}

// AddFromURL télécharge une image et la met en cache
func (c *Cache) AddFromURL(url string) (string, error) {
	resp, err := http.Get(url)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", errors.New("bad status")
	}
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}
	return c.Add(url, data)
}

// Add ajoute une image déjà téléchargée
func (c *Cache) Add(url string, data []byte) (string, error) {
	pk := pkFromURL(url)
	origPath := filepath.Join(c.dir, pk+".orig.webp")

	c.mu.Lock()
	defer c.mu.Unlock()

	if _, err := os.Stat(origPath); errors.Is(err, os.ErrNotExist) {
		img, _, err := image.Decode(bytes.NewReader(data))
		if err != nil {
			return "", err
		}
		buf, err := encodeWebP(img)
		if err != nil {
			return "", err
		}
		if err := os.WriteFile(origPath, buf, 0o644); err != nil {
			return "", err
		}
	}

	_ = c.db.Add(pk, url) // ajoute ou met à jour la base
	return pk, nil
}

// Get renvoie le chemin du fichier original et met à jour hits/last_used
func (c *Cache) Get(pk string) (string, error) {
	c.mu.Lock()
	defer c.mu.Unlock()

	_, err := c.db.Get(pk)
	if err != nil {
		return "", err
	}

	_ = c.db.UpdateHit(pk)

	origPath := filepath.Join(c.dir, pk+".orig.webp")
	if _, err := os.Stat(origPath); err != nil {
		return "", err
	}
	return origPath, nil
}

// Purge vide complètement le cache
func (c *Cache) Purge() error {
	c.mu.Lock()
	defer c.mu.Unlock()

	files, _ := filepath.Glob(filepath.Join(c.dir, "*"))
	for _, f := range files {
		os.Remove(f)
	}
	return c.db.Purge()
}

// pkFromURL → hash stable basé sur l’URL
func pkFromURL(url string) string {
	h := sha1.Sum([]byte(url))
	return hex.EncodeToString(h[:8])
}
