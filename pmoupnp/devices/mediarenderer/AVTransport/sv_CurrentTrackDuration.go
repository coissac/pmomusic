package avtransport

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var CurrentTrackDuration = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("CurrentTrackDuration")

	return ts
}()
