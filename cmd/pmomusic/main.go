package main

import (
	"context"
	"log"
	"os"
	"os/signal"
	"syscall"
	"time"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/internal/renderer"
	"gargoton.petite-maison-orange.fr/eric/pmomusic/internal/upnp"
)

func main() {
	// Crée le serveur avec baseURL auto-déduite depuis l’IP locale
	server := upnp.NewServer("PMO Music Server", "PMO Factory", "Fake Server", "", 1400)

	// Crée le renderer UPnP
	rendererDevice := renderer.NewMusicRenderer("pmomusic Fake Renderer", "pmomusic", "fake model")
	server.RegisterDevice("MusicRenderer", rendererDevice)

	// Lance le serveur HTTP
	if err := server.Start(); err != nil {
		log.Fatalf("Failed to start UPnP server: %v", err)
	}

	// Gère les signaux pour un arrêt propre
	sigs := make(chan os.Signal, 1)
	signal.Notify(sigs, syscall.SIGINT, syscall.SIGTERM)

	log.Println("UPnP MusicRenderer is running... Press Ctrl+C to stop.")
	<-sigs

	log.Println("Shutting down...")

	// Dé-annonce SSDP (optionnel)
	server.NotifyByeBye()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := server.Stop(ctx); err != nil {
		log.Printf("Error shutting down UPnP server: %v", err)
	}
}
