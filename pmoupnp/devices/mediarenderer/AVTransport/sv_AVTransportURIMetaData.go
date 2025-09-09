package avtransport

import (
	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmodidl"
	sv "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/statevariables"
	log "github.com/sirupsen/logrus"
)

func _AVTransportURIMetaDataParser(value string) (interface{}, error) {
	log.Debug("[avtransport] Parsing AVTransport)")
	didl, err := pmodidl.Parse(value)
	if err != nil {
		return value, err
	}

	return didl, nil
}

var AVTransportURIMetaData = func() *sv.StateVariable {

	ts := sv.StateType_String.NewStateValue("AVTransportURIMetaData")
	ts.SetValueParser(_AVTransportURIMetaDataParser)

	return ts
}()
