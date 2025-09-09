package avtransport

import "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/actions"

var SetAVTransportURI = func() *actions.Action {

	ac := actions.NewAction("SetAVTransportURI")
	ac.AddArgument(
		actions.NewInArgument(
			"InstanceID",
			A_ARG_TYPE_InstanceID,
		),
	)

	ac.AddArgument(
		actions.NewInArgument(
			"CurrentURI",
			AVTransportURI,
		),
	)

	ac.AddArgument(
		actions.NewInArgument(
			"CurrentURIMetaData",
			AVTransportURIMetaData,
		),
	)

	return ac
}()
