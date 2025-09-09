package connectionmanager

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/statevariables"

var CurrentConnectionIDs = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("CurrentConnectionIDs")

	return ts
}()
