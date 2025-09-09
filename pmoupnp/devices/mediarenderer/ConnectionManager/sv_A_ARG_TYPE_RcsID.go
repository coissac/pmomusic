package connectionmanager

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/statevariables"

var A_ARG_TYPE_RcsID = func() *sv.StateVariable {

	ts := sv.StateType_I4.NewStateValue("A_ARG_TYPE_RcsID")

	return ts
}()
