package upnp

import (
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"strings"
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

// decodeBinary decodes Base64 or Hex-encoded binary strings to byte slices
func decodeBinary(t StateVarType, val string) ([]byte, error) {
	switch t {
	case StateType_BinBase64:
		data, err := base64.StdEncoding.DecodeString(val)
		if err != nil {
			return nil, fmt.Errorf("invalid base64: %v", err)
		}
		return data, nil

	case StateType_BinHex:
		// Accept even-length hex string
		val = strings.TrimSpace(val)
		if len(val)%2 != 0 {
			return nil, fmt.Errorf("invalid hex: odd-length string")
		}
		data := make([]byte, len(val)/2)
		_, err := hex.Decode(data, []byte(val))
		if err != nil {
			return nil, fmt.Errorf("invalid hex: %v", err)
		}
		return data, nil

	default:
		return nil, fmt.Errorf("decodeBinary: unsupported binary type %v", t)
	}
}
