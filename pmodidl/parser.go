package pmodidl

import (
	"encoding/xml"
	"fmt"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmocover"
)

func Parse(metadata string) (*DIDLLite, error) {
	var didl DIDLLite
	err := xml.Unmarshal([]byte(metadata), &didl)
	if err != nil {
		return nil, fmt.Errorf("failed to parse DIDL-Lite: %v", err)

	}

	cache, err := pmocover.GetCoverCache()
	if err != nil {
		return &didl, fmt.Errorf("failed to get cover cache: %v", err)
	}

	didl.CacheAllCoverArts(cache)

	return &didl, nil
}
