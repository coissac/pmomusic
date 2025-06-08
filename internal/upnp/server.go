package upnp

import (
	"embed"
	"fmt"
	"io/fs"
	"log"
	"net/http"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/internal/soap"
)

//go:embed xml/*.xml
var embeddedXML embed.FS

//go:embed html/*.html
var embeddedHTML embed.FS

// ServeStaticXML mounts handlers for SCPD XML files.
func ServeStaticXML(mux *http.ServeMux) {
	subFS, err := fs.Sub(embeddedXML, "xml")
	if err != nil {
		panic("failed to create sub FS: " + err.Error())
	}
	mux.Handle("/scpd/", http.StripPrefix("/scpd/", http.FileServer(http.FS(subFS))))
}

func ServeStaticHTML(mux *http.ServeMux) {
	subFS, err := fs.Sub(embeddedHTML, "html")
	if err != nil {
		panic("failed to create sub FS: " + err.Error())
	}
	mux.Handle("/", http.FileServer(http.FS(subFS)))
}

func StartHTTPServer(device *DeviceDescription) {
	mux := http.NewServeMux()

	// Device description
	mux.HandleFunc("/description.xml", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/xml")
		xml, err := device.GenerateXML()
		if err != nil {
			w.WriteHeader(500)
			w.Write([]byte(err.Error()))
			return
		}

		w.Write([]byte(xml))
	})

	// SOAP control endpoints
	mux.HandleFunc("/upnp/control/AVTransport", soap.HandleSOAP)
	mux.HandleFunc("/upnp/control/RenderingControl", soap.HandleSOAP)
	mux.HandleFunc("/upnp/control/ConnectionManager", soap.HandleSOAP)

	// Serve static SCPD XML files
	ServeStaticXML(mux)
	ServeStaticHTML(mux)

	addr := fmt.Sprintf("%s:%d", device.IP, device.Port)
	log.Printf("Serving UPnP fake renderer at http://%s", addr)
	log.Fatal(http.ListenAndServe(addr, mux))
}
