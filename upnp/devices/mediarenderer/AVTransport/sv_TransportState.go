package mediarenderer

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"

var TransportState = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("TransportState")

	ts.SetAllowedValues(
		"STOPPED",
		"PLAYING",
		"PAUSED_PLAYBACK",
		"TRANSITIONING",
		"NO_MEDIA_PRESENT",
	)

	ts.SetSendingEvents()

	return ts
}()
