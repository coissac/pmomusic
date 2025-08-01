package mediarenderer

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var TransportStatus = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("TransportStatus")

	ts.SetAllowedValues(
		"OK",
		"ERROR_OCCURRED",
	)

	ts.SetSendingEvents()

	return ts
}()
