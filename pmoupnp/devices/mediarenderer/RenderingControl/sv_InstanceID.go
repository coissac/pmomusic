package renderingcontrol

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/statevariables"

var InstanceID = func() *sv.StateVariable {

	ts := sv.StateType_I4.NewStateValue("InstanceID")

	return ts
}()
