package connectionmanager

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var A_ARG_TYPE_Direction = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("A_ARG_TYPE_Direction")
	ts.AppendAllowedValue("Input", "Output")

	return ts
}()
