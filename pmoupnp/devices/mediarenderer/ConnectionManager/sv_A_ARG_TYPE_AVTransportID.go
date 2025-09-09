package connectionmanager

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var A_ARG_TYPE_AVTransportID = func() *sv.StateVariable {

	ts := sv.StateType_I4.NewStateValue("A_ARG_TYPE_AVTransportID")

	return ts
}()
