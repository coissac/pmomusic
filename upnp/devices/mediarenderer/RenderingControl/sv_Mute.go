package renderingcontrol

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var Mute = func() *sv.StateVariable {

	ts := sv.StateType_Boolean.NewStateValue("Mute")
	ts.SetSendingEvents()
	ts.SetDefault(false)

	return ts
}()
