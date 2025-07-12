package upnp

import (
	"fmt"
	"math"
	"strconv"
)

func maxFloat(bits int) float64 {
	switch bits {
	case 32:
		return float64(math.MaxFloat32)
	case 64:
		return math.MaxFloat64
	default:
		return math.MaxFloat64 // fallback
	}
}

func minFloat(bits int) float64 {
	switch bits {
	case 32:
		return -float64(math.MaxFloat32)
	case 64:
		return -math.MaxFloat64
	default:
		return -math.MaxFloat64 // fallback
	}
}

// toFloat converts various types to float (32 or 64 bits).
// Checks float32 boundaries when converting to 32-bit float.
// toFloat converts v to a float64, ensuring it fits within the range of the requested float size.
func toFloat(v interface{}, bits int) (float64, error) {
	var f float64

	switch val := v.(type) {
	case float32:
		f = float64(val)
	case float64:
		f = val
	case int:
		f = float64(val)
	case int8:
		f = float64(val)
	case int16:
		f = float64(val)
	case int32:
		f = float64(val)
	case int64:
		f = float64(val)
	case uint:
		f = float64(val)
	case uint8:
		f = float64(val)
	case uint16:
		f = float64(val)
	case uint32:
		f = float64(val)
	case uint64:
		f = float64(val)
	case string:
		var err error
		f, err = strconv.ParseFloat(val, bits)
		if err != nil {
			return 0, err
		}
	default:
		return 0, fmt.Errorf("%T,unsupported type in toFloat", v)
	}

	if f < minFloat(bits) || f > maxFloat(bits) {
		return 0, fmt.Errorf("value %v overflows float%d range", v, bits)
	}

	return f, nil
}

func cmpFloat64(a, b float64) int {
	switch {
	case a < b:
		return -1
	case a > b:
		return 1
	default:
		return 0
	}
}
