package stateVariables

import (
	"fmt"
)

// toString converts v to string if possible.
func toString(v interface{}) (string, error) {
	switch val := v.(type) {
	case string:
		return val, nil

	case []byte:
		return string(val), nil

	case fmt.Stringer:
		return val.String(), nil

	case int, int8, int16, int32, int64:
		return fmt.Sprintf("%d", val), nil

	case uint, uint8, uint16, uint32, uint64:
		return fmt.Sprintf("%d", val), nil

	case float32, float64:
		return fmt.Sprintf("%v", val), nil

	case bool:
		if val {
			return "true", nil
		}
		return "false", nil

	default:
		return "", fmt.Errorf("cannot convert type %T to string", v)
	}
}
