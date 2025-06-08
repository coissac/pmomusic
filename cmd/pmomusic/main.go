package main

import (
	log "github.com/sirupsen/logrus"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/internal/netutils"
	"gargoton.petite-maison-orange.fr/eric/pmomusic/internal/ssdp"
	"gargoton.petite-maison-orange.fr/eric/pmomusic/internal/upnp"
)

func main() {
	localIP, err := netutils.GuessLocalIP()

	if err != nil {
		log.Fatalf("Could not guess local IP: %v", err)
	}

	usn := ssdp.GetUUID()

	desc := upnp.NewDevice(localIP, 1400, usn, "pmomusic Fake Renderer", "pmomusic")

	desc.RegisterService("urn:schemas-upnp-org:service:AVTransport:1")
	desc.RegisterService("urn:schemas-upnp-org:service:RenderingControl:1")
	desc.RegisterService("urn:schemas-upnp-org:service:ConnectionManager:1")

	desc.RegisterService("urn:av-openhome-org:service:Product:1")
	desc.RegisterService("urn:av-openhome-org:service:Playlist:1")
	desc.RegisterService("urn:av-openhome-org:service:Info:1")

	location := "http://" + localIP + ":1400/description.xml"
	log.Printf("Server UUID: %s", usn)

	log.Info(desc.GenerateXML())
	// Démarre la diffusion SSDP
	go ssdp.AnnounceRenderer(desc)
	go ssdp.StartSSDPResponder(desc)

	// Lance le serveur HTTP
	log.Println("Starting HTTP server at:", location)
	upnp.StartHTTPServer(desc)
}
