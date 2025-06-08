package ssdp

import (
	"log"
	"time"

	"github.com/koron/go-ssdp"
)

func AnnounceRenderer(usn, location string) {
	st := "urn:schemas-upnp-org:device:MediaRenderer:1"
	server := "pmomusic/1.0 UPnP/1.1 DLNARenderer/1.0"
	maxAge := 1800

	_, err := ssdp.Advertise(st, usn+"::"+st, location, server, maxAge)
	if err != nil {
		log.Println("SSDP advertise error:", err)
		return
	}

	log.Println("✅ SSDP advertisement started")

	// Ce `Advertiser` fait automatiquement le NOTIFY loop.
	// Il est censé continuer à diffuser les `alive` tous les MaxAge / 2.

	// Exemple : on attend indéfiniment
	for {
		time.Sleep(time.Hour)
	}

	// Un jour on voudra faire ça à l'arrêt :
	// adv.Close() // envoie le ssdp:byebye
}
