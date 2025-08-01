package mediarenderer

import "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services"

var AVTransport = func() *services.Service {
	svc := services.NewService("AVTransport")

	svc.AddVariable(AVTransportURI)
	svc.AddVariable(AVTransportURIMetaData)
	svc.AddVariable(CurrentTrackDuration)
	svc.AddVariable(SeekMode)
	svc.AddVariable(TransportPlaySpeed)
	svc.AddVariable(TransportState)
	svc.AddVariable(TransportStatus)

	return svc
}()
