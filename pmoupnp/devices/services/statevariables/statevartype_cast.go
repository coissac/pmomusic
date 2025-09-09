package statevariables

import (
	"fmt"
	"net/url"
	"strings"
	"time"

	"github.com/google/uuid"
)

// Cast converts a value to the Go type corresponding to the UPnP type.
// Supports conversion from various primitive types and strings.
// Returns an error for unsupported conversions or invalid values.
//
// Examples:
//   - StateType_UI2.Cast(42)        // uint16(42), nil
//   - StateType_Boolean.Cast("true") // true, nil
//   - StateType_UI1.Cast(300)        // nil, error (overflow)
func (t StateVarType) Cast(val interface{}) (interface{}, error) {
	switch t {
	case StateType_UI1:
		v, err := toUint(val, 8)
		if err != nil {
			return nil, fmt.Errorf("cannot cast %v (%T) to UI1", val, val)
		}
		return uint8(v), nil

	case StateType_UI2:
		v, err := toUint(val, 16)
		if err != nil {
			return nil, fmt.Errorf("cannot cast %v (%T) to UI2", val, val)
		}
		return uint16(v), nil

	case StateType_UI4:
		v, err := toUint(val, 32)
		if err != nil {
			return nil, fmt.Errorf("cannot cast %v (%T) to UI4", val, val)
		}
		return uint32(v), nil

	case StateType_I1:
		v, err := toInt(val, 8)
		if err != nil {
			return nil, fmt.Errorf("cannot cast %v (%T) to I1", val, val)
		}
		return int8(v), nil

	case StateType_I2:
		v, err := toInt(val, 16)
		if err != nil {
			return nil, fmt.Errorf("cannot cast %v (%T) to I2", val, val)
		}
		return int16(v), nil

	case StateType_I4, StateType_Int:
		v, err := toInt(val, 32)
		if err != nil {
			return nil, fmt.Errorf("cannot cast %v (%T) to I4", val, val)
		}
		return int32(v), nil

	case StateType_R4:
		v, err := toFloat(val, 32)
		if err != nil {
			return nil, fmt.Errorf("cannot cast %v (%T) to R4", val, val)
		}
		return float32(v), nil

	case StateType_R8, StateType_Number, StateType_Fixed14_4:
		v, err := toFloat(val, 64)
		if err != nil {
			return nil, fmt.Errorf("cannot cast %v (%T) to R8", val, val)
		}
		return v, nil

	case StateType_Boolean:
		b, err := toBool(val)
		if err != nil {
			return nil, fmt.Errorf("cannot cast %v (%T) to Boolean", val, val)
		}
		return b, nil

	case StateType_Char:
		switch s := val.(type) {
		case string:
			if len(s) != 1 {
				return nil, fmt.Errorf("invalid Char: string too long %q", s)
			}
			return rune(s[0]), nil
		case rune:
			return s, nil
		default:
			return nil, fmt.Errorf("cannot cast %v (%T) to Char", val, val)
		}

	case StateType_String:
		return fmt.Sprint(val), nil

	case StateType_UUID:
		switch val := val.(type) {
		case uuid.UUID:
			return val, nil
		case string:
			u, err := uuid.Parse(strings.TrimSpace(val))
			if err != nil {
				return nil, fmt.Errorf("invalid UUID %v: %v", val, err)
			}
			return u, nil
		default:
			return nil, fmt.Errorf("cannot cast %v (%T) to UUID", val, val)
		}

	case StateType_URI:
		switch val := val.(type) {
		case *url.URL:
			return val, nil
		case string:
			u, err := url.Parse(strings.TrimSpace(val))
			if err != nil {
				return nil, fmt.Errorf("invalid URI %v: %v", val, err)
			}
			return u, nil
		default:
			return nil, fmt.Errorf("cannot cast %v (%T) to URI", val, val)
		}

	case StateType_BinBase64, StateType_BinHex:
		switch v := val.(type) {
		case []byte:
			return v, nil
		case string:
			return decodeBinary(t, v)
		default:
			return nil, fmt.Errorf("cannot cast %v (%T) to binary", val, val)
		}

	case StateType_Date, StateType_DateTime, StateType_DateTimeTZ,
		StateType_Time, StateType_TimeTZ:
		switch v := val.(type) {
		case time.Time:
			return v, nil
		case string:
			return parseUPnPTime(t, v)
		default:
			return nil, fmt.Errorf("cannot cast %v (%T) to time", val, val)
		}

	default:
		return nil, fmt.Errorf("unsupported type: %v", t)
	}
}
