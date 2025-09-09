package connectionmanager

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/statevariables"

var PeerConnectionID = func() *sv.StateVariable {

	ts := sv.StateType_I4.NewStateValue("PeerConnectionID")

	return ts
}()
