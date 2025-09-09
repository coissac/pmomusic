package avtransport

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/statevariables"

var A_ARG_TYPE_InstanceID = func() *sv.StateVariable {

	ts := sv.StateType_UI4.NewStateValue("A_ARG_TYPE_InstanceID")

	return ts
}()
