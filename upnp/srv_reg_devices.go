package upnp

import (
	log "github.com/sirupsen/logrus"
)

func (s *Server) RegisterDevice(name string, d *Device) {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.devices == nil {
		s.devices = make(DeviceInstanceSet)
	}

	if name == "" {
		name = d.Name()
	}

	config := GetConfig()
	udn := config.GetDeviceUDN(d.DeviceType(), name)

	instance := d.NewInstance(s, udn)

	log.Infof("✅ Registering device %s", name)

	err := s.devices.Insert(instance)

	if err != nil {
		log.Panicf("❌ Device  %s is already registered", instance.Name())
	}

	log.Infof("✅ New device %s get UDN : %s", instance.Name(), instance.UDN())

	// s.devices[name] = d
	// d.mu.Lock()
	// defer d.mu.Unlock()
	// d.UDN = s.UDN + "-" + name
	// d.Name = name

	// for _, service := range d.Services.All() {
	// 	service.DeviceName = name
	// }
}
