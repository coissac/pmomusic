package upnp

import (
	"fmt"
	"io"
	"net/http"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/soap"
	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/actions"
	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"
	"github.com/beevik/etree"
	log "github.com/sirupsen/logrus"
)

type ServiceInstance struct {
	name       string
	identifier string
	version    int

	device         *DeviceInstance
	statevariables statevariables.StateVarInstanceSet
	actions        actions.ActionInstanceSet
}

func (si *ServiceInstance) Name() string {
	return si.name
}

func (svc *ServiceInstance) TypeID() string {
	return "ServiceInstance"
}

func (si *ServiceInstance) Identifier() string {
	return si.identifier
}

func (svc *ServiceInstance) ServiceType() string {
	return fmt.Sprintf("urn:schemas-upnp-org:service:%s:%d", svc.name, svc.version)
}

func (svc *ServiceInstance) ServiceId() string {
	return fmt.Sprintf("urn:upnp-org:serviceId:%s", svc.identifier)
}

func (svc *ServiceInstance) BaseRoute() string {
	return fmt.Sprintf("%s/service/%s", svc.device.BaseRoute(), svc.Name())
}
func (svc *ServiceInstance) ControlURL() string {
	return fmt.Sprintf("%s/control", svc.BaseRoute())
}

func (svc *ServiceInstance) EventSubURL() string {
	return fmt.Sprintf("%s/event", svc.BaseRoute())
}

func (svc *ServiceInstance) SCPDURL() string {
	return fmt.Sprintf("%s/desc.xml", svc.BaseRoute())
}

func (svc *ServiceInstance) RegisterURLs() error {

	mux, ok := svc.device.server.httpSrv.Handler.(*http.ServeMux)

	if mux == nil || !ok {
		return fmt.Errorf("❌ Device %s the server handler is not correctly defined", svc.Name())
	}

	mux.HandleFunc(
		svc.SCPDURL(),
		svc.device.server.ServeXML(svc.SPCDElement),
	)

	mux.HandleFunc(
		svc.ControlURL(),
		svc.ControlHandler(),
	)

	mux.HandleFunc(
		svc.EventSubURL(),
		svc.EventSubHandler(),
	)

	log.Infof(
		"✅ Service description for %s:%s available at : %s%s",
		svc.device.Name(),
		svc.Name(),
		svc.device.server.BaseURL(),
		svc.SCPDURL(),
	)

	return nil
}

func (svc *ServiceInstance) USN() string {
	return fmt.Sprintf("uuid:%s::urn:%s", svc.device.UDN(), svc.ServiceType())
}

func (svc *ServiceInstance) SPCDElement() *etree.Element {
	elem := etree.NewElement("scpd")

	elem.CreateAttr("xmlns", "urn:schemas-upnp-org:service-1-0")

	spec := elem.CreateElement("specVersion")
	spec.CreateElement("major").SetText("1")
	spec.CreateElement("minor").SetText("0")

	if len(svc.actions) > 0 {
		elem.AddChild(svc.actions.ToXMLElement())
	}

	if len(svc.statevariables) > 0 {
		elem.AddChild(svc.statevariables.ToXMLElement())
	}

	return elem
}

func (svc *ServiceInstance) ToXMLElement() *etree.Element {
	elem := etree.NewElement("service")

	st := elem.CreateElement("serviceType")
	st.SetText(svc.ServiceType())

	sid := elem.CreateElement("serviceId")
	sid.SetText(svc.ServiceId())

	spcd := elem.CreateElement("SCPDURL")
	spcd.SetText(svc.SCPDURL())

	ctrl := elem.CreateElement("controlURL")
	ctrl.SetText(svc.ControlURL())

	event := elem.CreateElement("eventSubURL")
	event.SetText(svc.EventSubURL())

	return elem
}

func (svc *ServiceInstance) EventSubHandler() func(w http.ResponseWriter, r *http.Request) {
	return func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		log.Infof("Event Subscription handler for %s:%s", svc.device.Name(), svc.Name())
		// corps vide volontairement
	}
}

func (svc *ServiceInstance) ControlHandler() func(w http.ResponseWriter, r *http.Request) {
	return func(w http.ResponseWriter, r *http.Request) {
		log.Infof("📡 Control request for %s:%s", svc.device.Name(), svc.Name())
		log.Infof("➡️ Method: %s URL: %s", r.Method, r.URL.Path)
		log.Infof("Header SOAPACTION: %s", r.Header.Get("SOAPACTION"))
		log.Infof("Header Content-Type: %s", r.Header.Get("Content-Type"))

		body, err := io.ReadAll(r.Body)
		if err != nil {
			log.Errorf("❌ Failed to read body: %v", err)
			http.Error(w, "bad request", http.StatusBadRequest)
			return
		}

		soap.ParseSOAPGeneric(body)

		// Réponse minimale SOAP
		w.Header().Set("Content-Type", `text/xml; charset="utf-8"`)
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body/>
</s:Envelope>`))
	}
}
