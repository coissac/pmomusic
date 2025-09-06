package connectionmanager

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var Direction = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("Direction")
	ts.AppendAllowedValue("Input", "Ouput")

	return ts
}()
