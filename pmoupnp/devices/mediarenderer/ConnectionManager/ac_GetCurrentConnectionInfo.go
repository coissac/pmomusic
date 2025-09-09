package connectionmanager

import "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/actions"

var GetCurrentConnectionInfo = func() *actions.Action {

	ac := actions.NewAction("GetCurrentConnectionInfo")
	ac.AddArgument(
		actions.NewInArgument(
			"ConnectionIDs",
			A_ARG_TYPE_ConnectionID,
		),
	)

	ac.AddArgument(
		actions.NewOutArgument(
			"RcsID",
			A_ARG_TYPE_RcsID,
		),
	)

	ac.AddArgument(
		actions.NewOutArgument(
			"AVTransportID",
			A_ARG_TYPE_AVTransportID,
		),
	)

	ac.AddArgument(
		actions.NewOutArgument(
			"ProtocolInfo",
			A_ARG_TYPE_ProtocolInfo,
		),
	)

	ac.AddArgument(
		actions.NewOutArgument(
			"PeerConnectionManager",
			A_ARG_TYPE_ConnectionManager,
		),
	)

	ac.AddArgument(
		actions.NewOutArgument(
			"PeerConnectionID",
			A_ARG_TYPE_ConnectionID,
		),
	)

	ac.AddArgument(
		actions.NewOutArgument(
			"Direction",
			A_ARG_TYPE_Direction,
		),
	)

	ac.AddArgument(
		actions.NewOutArgument(
			"Status",
			A_ARG_TYPE_ConnectionStatus,
		),
	)
	return ac
}()
