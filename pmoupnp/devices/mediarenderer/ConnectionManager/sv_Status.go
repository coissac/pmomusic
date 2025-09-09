package connectionmanager

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var Status = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("Status")
	ts.AppendAllowedValue(
		"OK",
		"ContentFormatMismatch",
		"InsufficientBandwidth",
		"UnreliableChannel",
		"Unknown",
	)

	return ts
}()
