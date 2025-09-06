package avtransport

import "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/actions"

var Play = func() *actions.Action {

	ac := actions.NewAction("Play")
	ac.AddArgument(
		actions.NewInArgument(
			"InstanceID",
			A_ARG_TYPE_InstanceID,
		),
	)

	ac.AddArgument(
		actions.NewInArgument(
			"Speed",
			TransportPlaySpeed,
		),
	)

	return ac
}()
