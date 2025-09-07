package connectionmanager

import "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/actions"

var GetCurrentConnectionIDs = func() *actions.Action {

	ac := actions.NewAction("GetCurrentConnectionIDs")
	ac.AddArgument(
		actions.NewOutArgument(
			"ConnectionIDs",
			CurrentConnectionIDs,
		),
	)

	return ac
}()
