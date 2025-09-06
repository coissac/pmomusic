package upnp

import (
	"fmt"
)

type DeviceType string

const (
	MediaServer   DeviceType = "MediaServer"
	MediaRenderer DeviceType = "MediaRenderer"
)

type Device struct {
	name    string
	devtype DeviceType
	version int

	friendlyName     string
	manufacturer     string
	manufacturerURL  string
	modelDescription string
	modelName        string
	modelNumber      string
	modelURL         string
	serialNumber     string
	specVersion      string

	services ServiceSet
}

// NewDevice creates a new UPnP Device with the given name and type.
// It populates a minimal set of device attributes such as
// FriendlyName, Manufacturer, ModelName and a default version.
//
// Parameters:
//
//	name    – the human‑readable name of the device.
//	devtype – a unique string identifying the device type
//	          (used as the Device Identifier as well).
//
// Returns:
//
//	*Device – a pointer to the freshly allocated Device instance.
//	          The caller owns the reference and may further
//	          customise the device by setting additional fields
//	          or services.
//
// Side effects:
//
//	No I/O or external calls are performed. The function
//	simply constructs a struct in memory; it is safe to
//	call from multiple goroutines.
//
// Example:
//
//	// Create a speaker device and register it with a server.
//	dev := upnp.NewDevice("LivingRoomSpeaker", "AudioDevice")
//	server.RegisterDevice(dev.Name(), dev)
func NewDevice(name string, devtype DeviceType) *Device {
	switch devtype {
	case MediaServer, MediaRenderer:
		dev := &Device{
			name:         name,
			devtype:      devtype,
			friendlyName: "PMOMusic - " + name,
			manufacturer: "Petit Maison Orange",
			modelName:    "PMOMusic - " + name,
			version:      1,
			services:     make(ServiceSet),
		}
		return dev
	default:
		return nil
	}

}

func (d *Device) Name() string {
	return d.name
}

func (d *Device) SetName(name string) {
	d.name = name
}

func (d *Device) SetFriendlyName(name string) {
	d.friendlyName = name
}

func (d *Device) SetModelName(name string) {
	d.modelName = name
}

func (d *Device) TypeID() string {
	return "Device"
}

func (d *Device) DeviceType() DeviceType {
	return d.devtype
}

func (d *Device) SetVersion(version int) error {
	if version < 1 {
		return fmt.Errorf("%s", "version must be greater than or equal to 1")
	}
	d.version = version
	return nil
}

func (d *Device) Version() int {
	return d.version
}

func (d *Device) Manufacturer() string {
	return d.manufacturer
}

func (d *Device) SetManufacturer(manufacturer string) {
	d.manufacturer = manufacturer
}

func (d *Device) ModelName() string {
	return d.modelName
}

func (d *Device) AddService(srv *Service) error {
	err := d.services.Insert(srv)

	return err
}

func (d *Device) NewInstance(server *Server, udn string) *DeviceInstance {
	di := &DeviceInstance{
		name:             d.name,
		devtype:          d.devtype,
		version:          d.version,
		udn:              udn,
		server:           server,
		friendlyName:     d.friendlyName,
		manufacturer:     d.manufacturer,
		manufacturerURL:  d.manufacturerURL,
		modelDescription: d.modelDescription,
		modelName:        d.modelName,
		modelNumber:      d.modelNumber,
		modelURL:         d.modelURL,
		serialNumber:     d.serialNumber,
		specVersion:      d.specVersion,
		devices:          make(DeviceInstanceSet), // vide initialement
		services:         make(ServiceInstanceSet),
	}

	for svc := range d.services.All() {
		i := svc.NewInstance()
		i.device = di
		di.services.Insert(i)
	}

	return di
}
