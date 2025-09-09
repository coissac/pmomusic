package connectionmanager

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var PeerConnectionManager = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("PeerConnectionManager")

	return ts
}()
