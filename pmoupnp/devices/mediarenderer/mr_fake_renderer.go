package mediarenderer

import (
	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp"
	avtransport "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/mediarenderer/AVTransport"
	connectionmanager "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/mediarenderer/ConnectionManager"
	renderingcontrol "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/mediarenderer/RenderingControl"
)

var FakeRenderer = func() *pmoupnp.Device {

	renderer := pmoupnp.NewDevice("FakeRenderer", "MediaRenderer")

	renderer.AddService(avtransport.AVTransport)
	renderer.AddService(connectionmanager.ConnectionManager)
	renderer.AddService(renderingcontrol.RenderingControl)
	return renderer
}()
