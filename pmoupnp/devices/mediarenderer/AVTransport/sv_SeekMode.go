package avtransport

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var SeekMode = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("SeekMode")

	return ts
}()
