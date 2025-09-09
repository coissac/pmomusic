package connectionmanager

import sv "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/statevariables"

var SourceProtocolInfo = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("SourceProtocolInfo")

	return ts
}()
