package pmocover

import (
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// ServeMux branche les routes /covers
func (c *Cache) ServeMux(mux *http.ServeMux) {
	// Images
	mux.HandleFunc("/covers/images/", func(w http.ResponseWriter, r *http.Request) {
		parts := strings.Split(strings.TrimPrefix(r.URL.Path, "/covers/images/"), "/")
		if len(parts) == 0 || parts[0] == "" {
			http.Error(w, "missing pk", 400)
			return
		}
		pk := parts[0]
		if len(parts) == 1 {
			path, err := c.Get(pk)
			if err != nil {
				http.NotFound(w, r)
				return
			}
			w.Header().Set("Content-Type", "image/webp")
			http.ServeFile(w, r, path)
			return
		}
		size, err := strconv.Atoi(parts[1])
		if err != nil {
			http.Error(w, "invalid size", 400)
			return
		}
		variantPath := filepath.Join(c.dir, pk+"."+parts[1]+".webp")
		if _, err := os.Stat(variantPath); os.IsNotExist(err) {
			data, err := c.generateVariant(pk, size)
			if err != nil {
				http.Error(w, "cannot generate", 500)
				return
			}
			w.Header().Set("Content-Type", "image/webp")
			w.Write(data)
			return
		}
		w.Header().Set("Content-Type", "image/webp")
		http.ServeFile(w, r, variantPath)
	})

	// Stats
	mux.HandleFunc("/covers/stats", func(w http.ResponseWriter, r *http.Request) {
		entries, err := c.db.GetAll()
		if err != nil {
			http.Error(w, "cannot retrieve stats", 500)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(entries)
	})
}
