package mediarenderer

import (
	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp"
	avtransport "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/mediarenderer/AVTransport"
	connectionmanager "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/mediarenderer/ConnectionManager"
	renderingcontrol "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/mediarenderer/RenderingControl"
)

var FakeRenderer = func() *upnp.Device {

	renderer := upnp.NewDevice("FakeRenderer", "MediaRenderer")

	renderer.AddService(avtransport.AVTransport)
	renderer.AddService(connectionmanager.ConnectionManager)
	renderer.AddService(renderingcontrol.RenderingControl)
	return renderer
}()
