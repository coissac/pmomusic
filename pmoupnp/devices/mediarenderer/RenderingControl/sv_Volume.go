package renderingcontrol

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/statevariables"

var Volume = func() *sv.StateVariable {

	vol := sv.StateType_UI2.NewStateValue("Volume")

	vol.SetRange(0, 100)
	vol.SetStep(1)

	vol.SetSendingEvents()

	return vol
}()
