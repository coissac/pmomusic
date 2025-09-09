package avtransport

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/statevariables"

var TransportState = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("TransportState")

	ts.SetAllowedValues(
		"STOPPED",
		"PLAYING",
		"RECORDING",
		"TRANSITIONING",
		"PAUSED_PLAYBACK",
		"PAUSED_RECORDING",
		"NO_MEDIA_PRESENT",
	)

	ts.SetSendingEvents()

	return ts
}()
