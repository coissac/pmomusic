package services

import (
	"fmt"
	"iter"

	"github.com/beevik/etree"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/actions"
	sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"
)

type Service struct {
	name       string
	identifier string
	version    int

	controlURL  string
	eventSubURL string
	scpdURL     string

	actions    actions.ActionSet
	stateTable sv.StateVariableSet
}

func NewService(name string) *Service {
	svc := &Service{
		name:        name,
		identifier:  name,
		controlURL:  "/service/" + name + "/control",
		eventSubURL: "/service/" + name + "/event",
		scpdURL:     "/service/" + name + "/desc.xml",
		version:     1,
	}

	return svc
}

func (svc *Service) Name() string {
	return svc.name
}

func (svc *Service) TypeID() string {
	return "Service"
}

func (svc *Service) ServiceType() string {
	return fmt.Sprintf("urn:schemas-upnp-org:service:%s:%d", svc.name, svc.version)
}

func (svc *Service) ServiceId() string {
	return fmt.Sprintf("urn:upnp-org:serviceId:%s", svc.identifier)
}

func (svc *Service) SetIdentifier(id string) {
	svc.identifier = id
}

func (svc *Service) ControlURL() string {
	return svc.controlURL
}

func (svc *Service) SetControlURL(url string) {
	svc.controlURL = url
}

func (svc *Service) EventSubURL() string {
	return svc.eventSubURL
}

func (svc *Service) SetEventSubURL(url string) {
	svc.eventSubURL = url
}

func (svc *Service) SCPDURL() string {
	return svc.scpdURL
}

func (svc *Service) SetSCPDURL(url string) {
	svc.scpdURL = url
}

func (svc *Service) SetVersion(version int) error {
	if version < 1 {
		return fmt.Errorf("%s", "version must be greater than or equal to 1")
	}
	svc.version = version
	return nil
}

func (svc *Service) Version() int {
	return svc.version
}

func (svc *Service) AddVariable(sv *sv.StateVariable) {
	svc.stateTable.Insert(sv)
}

func (svc *Service) ContaintsVariable(sv *sv.StateVariable) bool {
	return svc.stateTable.Contains(sv)
}

func (svc *Service) Variables() iter.Seq[*sv.StateVariable] {
	return svc.stateTable.All()
}

func (svc *Service) AddAction(ac *actions.Action) {
}

func (svc *Service) ToXMLElement() *etree.Element {
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
