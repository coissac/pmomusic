package upnp

import (
	"fmt"
	"net/http"
	"runtime"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/ssdp"
	"github.com/beevik/etree"
	log "github.com/sirupsen/logrus"
)

type DeviceInstance struct {
	// Identification spécifique à l’instance
	name    string
	devtype DeviceType
	version int
	udn     string
	server  *Server

	// Copie figée des infos du Device
	friendlyName     string
	manufacturer     string
	manufacturerURL  string
	modelDescription string
	modelName        string
	modelNumber      string
	modelURL         string
	serialNumber     string
	specVersion      string

	// Sous-devices si le device en contient
	devices  DeviceInstanceSet
	services ServiceInstanceSet
}

func (di *DeviceInstance) Name() string {
	return di.name
}

func (di *DeviceInstance) TypeID() string {
	return "DeviceInstance"
}

func (di *DeviceInstance) DeviceType() DeviceType {
	return di.devtype
}

func (di *DeviceInstance) UDN() string {
	return di.udn
}

func (di *DeviceInstance) ServiceType() string {
	return fmt.Sprintf("urn:schemas-upnp-org:device:%s:%d", di.devtype, di.version)
}

func (di *DeviceInstance) FriendlyName() string {
	return di.friendlyName
}

func (di *DeviceInstance) Manufacturer() string {
	return di.manufacturer
}

func (di *DeviceInstance) ModelName() string {
	return di.modelName
}

func (di *DeviceInstance) BaseRoute() string {
	return fmt.Sprintf("/device/%s/%s", di.DeviceType(), di.UDN())
}
func (di *DeviceInstance) DescriptionURL() string {
	return fmt.Sprintf("%s/desc.xml", di.BaseRoute())
}

func (di *DeviceInstance) RegisterURLs() error {

	mux, ok := di.server.httpSrv.Handler.(*http.ServeMux)

	if mux == nil || !ok {
		return fmt.Errorf("❌ Device %s the server handler is not correctly defined", di.Name())
	}

	mux.HandleFunc(
		di.DescriptionURL(),
		di.server.ServeXML(di.ToXMLElement),
	)

	log.Infof(
		"✅ Device description for %s available at : %s%s",
		di.Name(),
		di.server.BaseURL(),
		di.DescriptionURL(),
	)

	for svc := range di.services.All() {
		err := svc.RegisterURLs()

		if err != nil {
			return fmt.Errorf(
				"❌ Service %s:%s URL error: %v",
				di.Name(),
				svc.Name(),
				err,
			)
		}
	}

	return nil
}

func (di *DeviceInstance) NT() string {
	return fmt.Sprintf("uuid:%s::urn:%s", di.UDN(), di.ServiceType())
}

func (di *DeviceInstance) RegisterSSPD() {

	osName := runtime.GOOS
	arch := runtime.GOARCH

	dev := &ssdp.Device{
		UUID:       di.UDN(),
		DeviceType: di.ServiceType(),
		Location:   fmt.Sprintf("%s%s", di.server.BaseURL(), di.DescriptionURL()),
		Server: fmt.Sprintf(
			"%s/%s UPnP/1.1 PMOMusic/1.0",
			osName, arch,
		),
		NTs: make([]string, 0, 2+len(di.services)),
	}

	dev.NTs = append(
		dev.NTs,
		"upnp:rootdevice",
		di.ServiceType(),
	)

	for s := range di.services.All() {
		dev.NTs = append(dev.NTs, s.ServiceType())
	}

	di.server.sspd.AddDevice(dev)
}

func (di *DeviceInstance) UnregisterSSPD() {
	di.server.sspd.RemoveDevice(di.UDN())
}

func (di *DeviceInstance) ToXMLElement() *etree.Element {
	elem := etree.NewElement("root")
	elem.CreateAttr("xmlns", "urn:schemas-upnp-org:device-1-0")

	spec := elem.CreateElement("specVersion")
	spec.CreateElement("major").SetText("1")
	spec.CreateElement("minor").SetText("0")

	device := elem.CreateElement("device")
	device.CreateElement("deviceType").SetText(di.ServiceType())
	device.CreateElement("friendlyName").SetText(di.FriendlyName())
	device.CreateElement("manufacturer").SetText(di.Manufacturer())
	device.CreateElement("modelName").SetText(di.ModelName())
	device.CreateElement("UDN").SetText("uuid:" + di.UDN())

	if len(di.services) > 0 {
		device.AddChild(di.services.ToXMLElement())
	}

	return elem
}

// // NewMediaRendererDescription génère la device description complète
// // comme *etree.Element (racine <root>).
// func NewMediaRendererDescription(udn string, friendlyName string) *etree.Element {

// 	// Inject serviceList
// 	device.AddChild(NewMediaRendererServiceList(udn))

// 	return root
// }
