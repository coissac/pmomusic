package stateVariables

import (
	"errors"
	"strings"
)

// parseUPnPBoolean parses a string like "true", "false", "1", "0" into a boolean.
func parseUPnPBoolean(s string) (bool, error) {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "1", "true":
		return true, nil
	case "0", "false":
		return false, nil
	default:
		return false, errors.New("invalid string for UPnP boolean")
	}
}

// toBool converts various types to boolean following UPnP rules:
// true: 1, "true"; false: 0, "false"
func toBool(val interface{}) (bool, error) {
	if val == nil {
		return false, errors.New("cannot convert nil to bool")
	}

	switch v := val.(type) {
	case bool:
		return v, nil

	case string:
		return parseUPnPBoolean(v)

	default:
		// try to convert numerics to float
		f, err := toFloat(v, 64)
		if err != nil {
			return false, err
		}

		if f == 1.0 {
			return true, nil
		}
		if f == 0.0 {
			return false, nil
		}
		return false, errors.New("numeric value cannot be converted to bool unless 0 or 1")
	}
}

// cmpBool compares two boolean values 'a' and 'b'. If they are equal it returns 0, if 'a' is false and 'b' is true it returns -1 else it returns 1.
func cmpBool(a, b bool) int {
	if a == b { // if both booleans are the same, return 0
		return 0
	}
	if !a && b { // if 'a' is false and 'b' is true, return -1
		return -1
	}
	// otherwise, return 1
	return 1 // if 'a' is true or 'a' and 'b' are not the same
}
