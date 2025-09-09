package renderingcontrol

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var Channel = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("Channel")
	ts.AppendAllowedValue("Master", "LF", "RF")

	return ts
}()
