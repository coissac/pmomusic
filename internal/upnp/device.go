package upnp

import (
	"bytes"
	"text/template"

	log "github.com/sirupsen/logrus"
)

type DeviceDescription struct {
	IP        string
	Port      uint
	USN       string
	Friendly  string
	ModelName string
	Services  *ServiceList
}

const deviceTemplate = `<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion>
    <major>1</major>
    <minor>0</minor>
  </specVersion>
  <URLBase>http://{{.IP}}:{{.Port}}/</URLBase>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>{{.Friendly}}</friendlyName>
    <manufacturer>GoDLNA</manufacturer>
    <manufacturerURL>http://example.com</manufacturerURL>
    <modelDescription>Fake DLNA Renderer</modelDescription>
    <modelName>{{.ModelName}}</modelName>
    <modelNumber>1.0</modelNumber>
    <modelURL>http://example.com/model</modelURL>
    <UDN>{{.USN}}</UDN>
    <presentationURL>http://{{.IP}}:{{.Port}}/</presentationURL>
    {{.ServicesXML}}
  </device>
</root>`

func NewDevice(ip string, port uint, usn string, friendly string, modelname string) *DeviceDescription {
	services := NewServiceList()
	return &DeviceDescription{
		IP:        ip,
		Port:      port,
		USN:       usn,
		Friendly:  friendly,
		ModelName: modelname,
		Services:  services,
	}
}

func (d *DeviceDescription) RegisterService(service string) {
	d.Services.Append(service)
}

func (d *DeviceDescription) GenerateXML() (string, error) {
	servicesXML, err := d.Services.GenerateServiceListXML()
	if err != nil {
		log.Errorf("Failed to generate service list XML: %v", err)
		return "", err
	}

	tmpl, err := template.New("device").Parse(deviceTemplate)
	if err != nil {
		log.Errorf("Failed to parse device description template: %v", err)
		return "", err
	}

	data := struct {
		IP          string
		Port        uint
		USN         string
		Friendly    string
		ModelName   string
		ServicesXML string // trusted raw XML
	}{
		IP:          d.IP,
		Port:        d.Port,
		USN:         d.USN,
		Friendly:    d.Friendly,
		ModelName:   d.ModelName,
		ServicesXML: servicesXML,
	}

	var buf bytes.Buffer
	if err := tmpl.Execute(&buf, data); err != nil {
		log.Errorf("Failed to execute template: %v", err)
		return "", err
	}

	return buf.String(), nil
}
