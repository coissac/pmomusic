package avtransport

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var AVTransportURI = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("AVTransportURI")

	return ts
}()
