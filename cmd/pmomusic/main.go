package main

import (
	"log"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/internal/ssdp"
	"gargoton.petite-maison-orange.fr/eric/pmomusic/internal/upnp"
)

func main() {
	localIP := ssdp.GetLocalIP()
	location := "http://" + localIP + ":1400/description.xml"
	usn := "uuid:pmomusic-renderer-001"

	// Démarre la diffusion SSDP
	go ssdp.AnnounceRenderer(usn, location)
	go ssdp.StartSSDPResponder(usn, location)

	// Lance le serveur HTTP
	log.Println("Starting HTTP server at:", location)
	upnp.StartHTTPServer(usn)
}
