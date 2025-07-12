package upnp

import (
	"encoding/base64"
	"encoding/hex"
	"fmt"
)

// toBinary tries to convert v into a []byte.
// - if v is []byte, returns it directly
// - if v is string, attempts base64 decode, then hex decode
func toBinary(v interface{}) ([]byte, error) {
	switch val := v.(type) {
	case []byte:
		return val, nil

	case string:
		// Try base64 first
		if data, err := base64.StdEncoding.DecodeString(val); err == nil {
			return data, nil
		}

		// Try hex next
		if data, err := hex.DecodeString(val); err == nil {
			return data, nil
		}

		return nil, fmt.Errorf("cannot parse string as base64 or hex: %q", val)

	default:
		return nil, fmt.Errorf("cannot convert type %T to binary", v)
	}
}
