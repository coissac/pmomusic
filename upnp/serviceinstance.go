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

		body, err := io.ReadAll(r.Body)
		if err != nil {
			log.Errorf("❌ Failed to read body: %v", err)
			http.Error(w, "bad request", http.StatusBadRequest)
			return
		}

		// Callback générique pour décoder chaque paramètre
		decoder := func(action, param, value string) (string, interface{}, error) {
			// 1️⃣ Chercher si le param correspond à une StateVarInstance

			sv, ok := svc.statevariables[param]
			log.Debugf("Look for a variable named : %s -> %v", param, ok)

			if ok {
				if sv.HasParser() {
					v, err := sv.ParseValue(value)
					if err != nil {
						return param, value, err
					}
					return param, v, err
				}
				v, err := sv.Cast(value)
				if err != nil {
					return param, value, err
				}
				return param, v, nil
			}

			// 2️⃣ Chercher si le param correspond à une ActionInstance et appliquer un parseur associé
			act, ok := svc.actions[action]
			log.Debugf("Look for an action named  :  %s -> %v", action, ok)

			if ok {
				if argument, ok := act.Arguments(param); ok {
					sv_name := argument.StateVariable().Name()
					sv := svc.statevariables[sv_name]
					v, err := sv.ParseValue(value)

					log.Debugf("It corresponds to variable : %s with a parser %v", sv_name, sv.HasParser())

					if err != nil {
						return param, value, err
					}
					return param, v, nil
				}
			}

			// 3️⃣ Sinon, valeur brute
			return param, value, nil
		}

		env, err := soap.ParseSOAPEnvelope(body)

		if err != nil {
			log.Errorf("❌ Failed to parse SOAP enveloppe: %v", err)
			soapResp, _ := soap.BuildSOAPFault("s:Client", "Invalid Args", err.Error())
			w.Header().Set("Content-Type", `text/xml; charset="utf-8"`)
			w.WriteHeader(http.StatusInternalServerError)
			_, _ = w.Write(soapResp)
			return
		}

		req, err := soap.ParseUPnPAction(env, decoder)
		if err != nil {
			log.Errorf("❌ Failed to parse SOAP Action: %v", err)
			soapResp, _ := soap.BuildSOAPFault("s:Client", "Invalid Args", err.Error())
			w.Header().Set("Content-Type", `text/xml; charset="utf-8"`)
			w.WriteHeader(http.StatusInternalServerError)
			_, _ = w.Write(soapResp)
			return
		}

		log.Info(req.ToMarkdown())

		// Ici tu peux appeler l'action correspondante sur svc.actions[req.Name] et récupérer le résultat
		// Exemple de réponse minimale :
		resp, _ := soap.BuildUPnPResponse(svc.ServiceType(), req.Name, map[string]string{})
		w.Header().Set("Content-Type", `text/xml; charset="utf-8"`)
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(resp)
	}
}
