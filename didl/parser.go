package didl

import (
	"encoding/xml"
	"fmt"
)

func Parse(metadata string) (*DIDLLite, error) {
	var didl DIDLLite
	err := xml.Unmarshal([]byte(metadata), &didl)
	if err != nil {
		return nil, fmt.Errorf("failed to parse DIDL-Lite: %v", err)

	}

	return &didl, nil
}
