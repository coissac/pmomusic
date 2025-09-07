package main

import (
	"context"
	"os/signal"
	"syscall"
	"time"

	log "github.com/sirupsen/logrus"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp"
	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/mediarenderer"
)

func main() {
	log.SetLevel(log.DebugLevel) // → niveau debug

	ctx, stop := signal.NotifyContext(
		context.Background(),
		syscall.SIGINT,
		syscall.SIGTERM,
	)
	defer stop()

	// Crée le serveur avec baseURL auto-déduite depuis l’IP locale
	server := upnp.NewServer("pmomusic")

	server.RegisterDevice("", mediarenderer.FakeRenderer)

	if err := server.Run(ctx); err != nil {
		log.Fatalf("server error: %v", err)
	}

	time.Sleep(2 * time.Second)

}
