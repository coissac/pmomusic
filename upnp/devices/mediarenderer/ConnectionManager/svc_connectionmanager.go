package connectionmanager

import "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp"

var ConnectionManager = func() *upnp.Service {
	svc := upnp.NewService("ConnectionManager")

	svc.AddVariable(SourceProtocolInfo)
	svc.AddVariable(SinkProtocolInfo)
	svc.AddVariable(SourceProtocolInfo)
	svc.AddVariable(CurrentConnectionIDs)
	svc.AddVariable(AVTransportID)
	svc.AddVariable(RcsID)
	svc.AddVariable(PeerConnectionManager)
	svc.AddVariable(PeerConnectionID)
	svc.AddVariable(Direction)
	svc.AddVariable(Status)
	svc.AddVariable(ConnectionID)

	return svc
}()
