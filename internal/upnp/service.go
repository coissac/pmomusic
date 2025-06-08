package upnp

import (
	"bytes"
	"fmt"
	"strings"
	"text/template"
)

type ServiceGroup string

const (
	UPnP     ServiceGroup = "upnp"
	OpenHome ServiceGroup = "openhome"
	Other    ServiceGroup = "other"
)

type ServiceDescription struct {
	Group     ServiceGroup
	Type      string
	Name      string
	ID        string
	URLPrefix string
	XML       string // auto-rempli temporairement pour le template parent
}

const serviceTemplate = `
<service>
  <serviceType>{{.Type}}</serviceType>
  <serviceId>{{.ID}}</serviceId>
  <controlURL>{{.URLPrefix}}/control</controlURL>
  <eventSubURL>{{.URLPrefix}}/event</eventSubURL>
  <SCPDURL>{{.URLPrefix}}.xml</SCPDURL>
</service>
`

func NewServiceDescription(serviceType string) *ServiceDescription {
	parts := strings.Split(serviceType, ":")
	name := parts[len(parts)-2]

	var group ServiceGroup
	switch {
	case strings.Contains(serviceType, "schemas-upnp-org"):
		group = UPnP
	case strings.Contains(serviceType, "av-openhome-org"):
		group = OpenHome
	default:
		group = Other
	}

	id := fmt.Sprintf("urn:%s:serviceId:%s", string(group), name)
	urlPrefix := fmt.Sprintf("/%s/%s", group, name)

	return &ServiceDescription{
		Group:     group,
		Type:      serviceType,
		Name:      name,
		ID:        id,
		URLPrefix: urlPrefix,
	}
}

func (desc *ServiceDescription) GenerateServiceXML() (string, error) {
	tmpl, err := template.New("service").Parse(strings.TrimSpace(serviceTemplate))
	if err != nil {
		return "", err
	}
	var buf bytes.Buffer
	if err := tmpl.Execute(&buf, desc); err != nil {
		return "", err
	}
	return buf.String(), nil
}

type ServiceList struct {
	Services []*ServiceDescription
}

func NewServiceList() *ServiceList {
	return &ServiceList{
		Services: make([]*ServiceDescription, 0),
	}
}

func (sl *ServiceList) Append(serviceType string) {
	svc := NewServiceDescription(serviceType)
	sl.Services = append(sl.Services, svc)
}

func (sl *ServiceList) GenerateServiceListXML() (string, error) {
	const listTemplate = `
<serviceList>
{{range .Services}}{{.XML}}
{{end}}</serviceList>`

	// Génère tous les XML individuellement
	for _, svc := range sl.Services {
		xml, err := svc.GenerateServiceXML()
		if err != nil {
			return "", err
		}
		svc.XML = xml
	}

	// Template principal
	tmpl, err := template.New("list").Parse(strings.TrimSpace(listTemplate))
	if err != nil {
		return "", err
	}
	var buf bytes.Buffer
	if err := tmpl.Execute(&buf, sl); err != nil {
		return "", err
	}
	return buf.String(), nil
}
