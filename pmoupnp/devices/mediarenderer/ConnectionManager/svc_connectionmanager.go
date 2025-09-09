package connectionmanager

import "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp"

var ConnectionManager = func() *pmoupnp.Service {
	svc := pmoupnp.NewService("ConnectionManager")

	svc.AddVariable(AVTransportID)
	svc.AddVariable(ConnectionID)
	svc.AddVariable(CurrentConnectionIDs)
	svc.AddVariable(Direction)
	svc.AddVariable(PeerConnectionID)
	svc.AddVariable(PeerConnectionManager)
	svc.AddVariable(RcsID)
	svc.AddVariable(SinkProtocolInfo)
	svc.AddVariable(SourceProtocolInfo)
	svc.AddVariable(Status)

	svc.AddVariable(A_ARG_TYPE_AVTransportID)
	svc.AddVariable(A_ARG_TYPE_ConnectionID)
	svc.AddVariable(A_ARG_TYPE_ConnectionManager)
	svc.AddVariable(A_ARG_TYPE_ConnectionStatus)
	svc.AddVariable(A_ARG_TYPE_Direction)
	svc.AddVariable(A_ARG_TYPE_ProtocolInfo)
	svc.AddVariable(A_ARG_TYPE_RcsID)

	svc.AddAction(GetCurrentConnectionIDs)
	svc.AddAction(GetCurrentConnectionInfo)
	svc.AddAction(GetProtocolInfo)

	return svc
}()
