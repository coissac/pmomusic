package renderingcontrol

import "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp"

var RenderingControl = func() *pmoupnp.Service {
	svc := pmoupnp.NewService("RenderingControl")

	svc.AddVariable(InstanceID)
	svc.AddVariable(Channel)
	svc.AddVariable(Mute)
	svc.AddVariable(Volume)

	return svc
}()
