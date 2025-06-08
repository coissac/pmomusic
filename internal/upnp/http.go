package upnp

import (
	"fmt"
	"log"
	"net/http"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/internal/soap"
)

func StartHTTPServer(usn string) {
	http.HandleFunc("/description.xml", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/xml")
		w.Write([]byte(generateDeviceDescription(usn)))
	})

	http.HandleFunc("/upnp/control/AVTransport", soap.HandleSOAP)

	log.Fatal(http.ListenAndServe(":1400", nil))
}

func generateDeviceDescription(usn string) string {
	return fmt.Sprintf(`<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion>
    <major>1</major>
    <minor>0</minor>
  </specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>pmomusic Fake Renderer</friendlyName>
    <manufacturer>GoDLNA</manufacturer>
    <manufacturerURL>http://example.com</manufacturerURL>
    <modelDescription>Fake DLNA Renderer</modelDescription>
    <modelName>pmomusic</modelName>
    <modelNumber>1.0</modelNumber>
    <modelURL>http://example.com/model</modelURL>
    <UDN>%s</UDN>
    <presentationURL>http://%s</presentationURL>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:AVTransport</serviceId>
        <controlURL>/upnp/control/AVTransport</controlURL>
        <eventSubURL>/upnp/event/AVTransport</eventSubURL>
        <SCPDURL>/scpd/AVTransport.xml</SCPDURL>
      </service>
      <service>
        <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:RenderingControl</serviceId>
        <controlURL>/upnp/control/RenderingControl</controlURL>
        <eventSubURL>/upnp/event/RenderingControl</eventSubURL>
        <SCPDURL>/scpd/RenderingControl.xml</SCPDURL>
      </service>
      <service>
        <serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:ConnectionManager</serviceId>
        <controlURL>/upnp/control/ConnectionManager</controlURL>
        <eventSubURL>/upnp/event/ConnectionManager</eventSubURL>
        <SCPDURL>/scpd/ConnectionManager.xml</SCPDURL>
      </service>
    </serviceList>
  </device>
</root>`, usn, "127.0.0.1:1400") // à adapter à ton IP:port réel
}
