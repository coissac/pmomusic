package connectionmanager

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var A_ARG_TYPE_ConnectionStatus = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("A_ARG_TYPE_ConnectionStatus")

	ts.AppendAllowedValue(
		"OK",
		"ContentFormatMismatch",
		"InsufficientBandwidth",
		"UnreliableChannel",
		"Unknown",
	)
	return ts
}()
