package connectionmanager

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/statevariables"

var ConnectionID = func() *sv.StateVariable {

	ts := sv.StateType_I4.NewStateValue("ConnectionID")

	return ts
}()
