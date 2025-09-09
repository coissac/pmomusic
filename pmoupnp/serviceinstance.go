package pmoupnp

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"time"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmolog"
	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/actions"
	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/statevariables"
	"gargoton.petite-maison-orange.fr/eric/pmomusic/soap"
	"github.com/beevik/etree"
	"github.com/google/uuid"
	log "github.com/sirupsen/logrus"
)

type ServiceInstance struct {
	name       string
	identifier string
	version    int

	device         *DeviceInstance
	statevariables statevariables.StateVarInstanceSet
	actions        actions.ActionInstanceSet
	subscribers    map[string]string // SID → Callback URL
	changedBuffer  map[string]interface{}
	seqid          map[string]uint32
	cbmu           sync.Mutex
	mu             sync.Mutex
}

const (
	MethodSubscribe   = "SUBSCRIBE"
	MethodUnsubscribe = "UNSUBSCRIBE"
)

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

func (svc *ServiceInstance) AddSubscriber(sid, callback string) {
	svc.mu.Lock()
	defer svc.mu.Unlock()
	if svc.subscribers == nil {
		svc.subscribers = make(map[string]string)
	}
	svc.subscribers[sid] = callback
}

func (svc *ServiceInstance) RenewSubscriber(sid, timeout string) {
	// Pour l'instant juste log, on peut étendre avec expiration
	svc.mu.Lock()
	defer svc.mu.Unlock()
	log.Infof("Renewed SID %s for timeout %s", sid, timeout)
}

func (svc *ServiceInstance) RemoveSubscriber(sid string) {
	svc.mu.Lock()
	defer svc.mu.Unlock()
	delete(svc.subscribers, sid)
}

// Envoi d'un événement initial (optionnel)
func (svc *ServiceInstance) SendInitialEvent(sid string) {
	svc.mu.Lock()
	callback := svc.subscribers[sid]
	svc.mu.Unlock()
	if callback == "" {
		return
	}

	changed := make(map[string]interface{})
	for name, sv := range svc.statevariables {
		if sv.IsSendingEvents() { // sendEvents="yes"
			changed[name] = sv.Value()
		}
	}

	if len(changed) == 0 {
		return
	}

	go func() {
		callback = strings.TrimSpace(callback)
		callback = strings.Trim(callback, "<>")

		body := `<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">`
		for name, val := range changed {
			body += fmt.Sprintf("<e:property><%s>%v</%s></e:property>", name, val, name)
		}
		body += "</e:propertyset>"

		req, _ := http.NewRequest("NOTIFY", callback, strings.NewReader(body))
		req.Header.Set("Content-Type", `text/xml; charset="utf-8"`)
		req.Header.Set("NT", "upnp:event")
		req.Header.Set("NTS", "upnp:propchange")
		req.Header.Set("SID", sid)
		req.Header.Set("SEQ", "0") // initial event

		client := &http.Client{}
		resp, err := client.Do(req)
		if err != nil {
			log.Errorf("Failed to send initial event to %s: %v", callback, err)
			return
		}
		defer resp.Body.Close()
		log.Infof(
			"✅ Initial event sent to %s, status=%s\n<details>\n\n```xml\n%s\n```\n</details>\n",
			callback,
			resp.Status,
			pmolog.PrettyPrintXML(body),
		)
	}()
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

func (svc *ServiceInstance) EventToBeSent(name string, value interface{}) {
	svc.cbmu.Lock()
	defer svc.cbmu.Unlock()

	svc.changedBuffer[name] = value
}

func (svc *ServiceInstance) nextSeq(sid string) string {
	svc.mu.Lock()
	defer svc.mu.Unlock()

	svc.seqid[sid]++
	return fmt.Sprintf("%d", svc.seqid[sid])
}

func (svc *ServiceInstance) NotifySubscribers() {
	svc.cbmu.Lock()
	if len(svc.subscribers) == 0 || len(svc.changedBuffer) == 0 {
		svc.cbmu.Unlock()
		return
	}

	// Copier et réinitialiser le buffer
	changed := svc.changedBuffer
	svc.changedBuffer = make(map[string]interface{})
	svc.cbmu.Unlock()

	for sid, callback := range svc.subscribers {
		go func(sid, callback string, changed map[string]interface{}) {
			callback = strings.TrimSpace(callback)
			callback = strings.Trim(callback, "<>")

			u, err := url.Parse(callback)
			if err != nil {
				log.Errorf("Invalid callback URL %s: %v", callback, err)
				return
			}

			body := `<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">`
			for name, val := range changed {
				body += fmt.Sprintf("<e:property><%s>%v</%s></e:property>", name, val, name)
			}
			body += "</e:propertyset>"

			req, err := http.NewRequest("NOTIFY", u.String(), strings.NewReader(body))
			if err != nil {
				log.Errorf("Failed to create NOTIFY request to %s: %v", callback, err)
				return
			}

			req.Header.Set("Content-Type", `text/xml; charset="utf-8"`)
			req.Header.Set("NT", "upnp:event")
			req.Header.Set("NTS", "upnp:propchange")
			req.Header.Set("SID", sid)
			req.Header.Set("SEQ", svc.nextSeq(sid))

			client := &http.Client{}
			resp, err := client.Do(req)
			if err != nil {
				log.Errorf("Failed to notify subscriber %s: %v", callback, err)
				return
			}
			defer resp.Body.Close()

			log.Infof("✅ Notified subscriber %s of changes: %v", callback, changed)
		}(sid, callback, changed)
	}
}

func (svc *ServiceInstance) StartNotifier(ctx context.Context, interval time.Duration) {
	log.Infof("✅ Starting notifier for %s:%s every %.2f s", svc.device.Name(), svc.Name(), interval.Seconds())
	go func() {
		ticker := time.NewTicker(interval)
		defer ticker.Stop()

		for {
			select {
			case <-ctx.Done():
				log.Infof("✅ Notifier stopped for %s:%s", svc.device.Name(), svc.Name())
				return
			case <-ticker.C:
				svc.NotifySubscribers()
			}
		}
	}()
}

func (svc *ServiceInstance) EventSubHandler() func(w http.ResponseWriter, r *http.Request) {
	return func(w http.ResponseWriter, r *http.Request) {
		log.Infof("📡 Event Subscription request for %s:%s", svc.device.Name(), svc.Name())

		sid := r.Header.Get("SID")
		timeout := r.Header.Get("Timeout")
		callback := r.Header.Get("Callback")

		switch r.Method {
		case MethodSubscribe:
			if sid == "" {
				// Nouvelle subscription
				sid = fmt.Sprintf("uuid:%s", uuid.New().String())
				if callback != "" {
					svc.AddSubscriber(sid, callback)
				}
				if timeout == "" {
					timeout = "Second-1800"
				}
				log.Infof("🔔 New subscription: SID=%s, Callback=%s, Timeout=%s", sid, callback, timeout)
				go svc.SendInitialEvent(sid) // envoyer l’état initial
			} else {
				// Renouvellement
				svc.RenewSubscriber(sid, timeout)
				log.Infof("♻️ Renew subscription: SID=%s, Timeout=%s", sid, timeout)
			}

			w.Header().Set("SID", sid)
			w.Header().Set("Timeout", timeout)
			w.WriteHeader(http.StatusOK)

		case MethodUnsubscribe:
			if sid != "" {
				svc.RemoveSubscriber(sid)
				log.Infof("❌ Unsubscribe SID=%s", sid)
			}
			w.WriteHeader(http.StatusOK)

		default:
			log.Warnf("Unsupported EventSub method: %s", r.Method)
			http.Error(w, "Method Not Allowed", http.StatusMethodNotAllowed)
		}

		// Debug log headers
		for k, v := range r.Header {
			log.Debugf("Header: %s=%v", k, v)
		}

		// Lire le body au besoin
		body, err := io.ReadAll(r.Body)
		if err == nil && len(body) > 0 {
			log.Debugf("Body: %s", string(body))
		}
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
