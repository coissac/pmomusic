package renderingcontrol

import "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp"

var RenderingControl = func() *upnp.Service {
	svc := upnp.NewService("RenderingControl")

	svc.AddVariable(InstanceID)
	svc.AddVariable(Channel)
	svc.AddVariable(Mute)
	svc.AddVariable(Volume)

	return svc
}()
