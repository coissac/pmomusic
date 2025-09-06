package avtransport

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var A_ARG_TYPE_PlaySpeed = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("A_ARG_TYPE_PlaySpeed")

	return ts
}()
