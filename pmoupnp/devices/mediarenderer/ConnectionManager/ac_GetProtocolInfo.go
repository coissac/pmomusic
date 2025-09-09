package connectionmanager

import "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/actions"

var GetProtocolInfo = func() *actions.Action {

	ac := actions.NewAction("GetProtocolInfo")
	ac.AddArgument(
		actions.NewOutArgument(
			"Source",
			SourceProtocolInfo,
		),
	)

	ac.AddArgument(
		actions.NewOutArgument(
			"Sink",
			SinkProtocolInfo,
		),
	)

	return ac
}()
