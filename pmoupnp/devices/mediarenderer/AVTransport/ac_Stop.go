package avtransport

import "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/actions"

var Stop = func() *actions.Action {

	ac := actions.NewAction("Stop")
	ac.AddArgument(
		actions.NewInArgument(
			"InstanceID",
			A_ARG_TYPE_InstanceID,
		),
	)

	return ac
}()
