package mediarenderer

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var AVTransportURIMetaData = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("AVTransportURIMetaData")

	return ts
}()
