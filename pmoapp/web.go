package pmoapp

import (
	"embed"
	"io/fs"
	"net/http"
)

//go:embed web/dist/*
var AppRoot embed.FS

func Handler(mux *http.ServeMux) {
	fsys, _ := fs.Sub(AppRoot, "web/dist")

	// /app → page principale
	mux.Handle("/app/", http.StripPrefix("/app/", http.FileServer(http.FS(fsys))))
	mux.HandleFunc("/app", func(w http.ResponseWriter, r *http.Request) {
		index, _ := fs.ReadFile(fsys, "index.html")
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		w.Write(index)
	})

	// /log → interface log (React)
	mux.Handle("/log/", http.StripPrefix("/log/", http.FileServer(http.FS(fsys))))
	mux.HandleFunc("/log", func(w http.ResponseWriter, r *http.Request) {
		index, _ := fs.ReadFile(fsys, "index.html")
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		w.Write(index)
	})

	// / → redirect vers /app
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, "/app", http.StatusFound)
	})
}
