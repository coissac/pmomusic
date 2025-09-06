package avtransport

import "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp"

var AVTransport = func() *upnp.Service {
	svc := upnp.NewService("AVTransport")

	svc.AddAction(SetAVTransportURI)
	svc.AddAction(Play)
	svc.AddAction(Stop)

	svc.AddVariable(A_ARG_TYPE_InstanceID)
	svc.AddVariable(A_ARG_TYPE_PlaySpeed)
	svc.AddVariable(AVTransportURI)
	svc.AddVariable(AVTransportURIMetaData)
	svc.AddVariable(CurrentTrackDuration)
	svc.AddVariable(SeekMode)
	svc.AddVariable(TransportPlaySpeed)
	svc.AddVariable(TransportState)
	svc.AddVariable(TransportStatus)

	return svc
}()
