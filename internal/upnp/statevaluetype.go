// Package upnp provides comprehensive handling of UPnP state variable types.
// It includes type identification, value casting, comparison, and range validation
// for all standard UPnP state variable types.
package upnp

import (
	"bytes"
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"log"
	"math"
	"net/url"
	"reflect"
	"strconv"
	"strings"
	"time"

	"github.com/google/uuid"
)

// StateVarType represents UPnP state variable types with corresponding Go type mappings.
type StateVarType int

// Constants defining all supported UPnP state variable types
const (
	StateType_Unknown    StateVarType = iota
	StateType_UI1                     // Unsigned 8-bit integer (Go: uint8)
	StateType_UI2                     // Unsigned 16-bit integer (Go: uint16)
	StateType_UI4                     // Unsigned 32-bit integer (Go: uint32)
	StateType_I1                      // Signed 8-bit integer (Go: int8)
	StateType_I2                      // Signed 16-bit integer (Go: int16)
	StateType_I4                      // Signed 32-bit integer (Go: int32)
	StateType_Int                     // Synonymous with i4 (Go: int32)
	StateType_R4                      // 32-bit floating point (Go: float32)
	StateType_R8                      // 64-bit floating point (Go: float64)
	StateType_Number                  // Synonymous with r8 (Go: float64)
	StateType_Fixed14_4               // Fixed-point decimal (Go: float64)
	StateType_Char                    // Single Unicode character (Go: rune)
	StateType_String                  // Character string (Go: string)
	StateType_Boolean                 // Boolean value (Go: bool)
	StateType_BinBase64               // Base64-encoded binary (Go: []byte)
	StateType_BinHex                  // Hex-encoded binary (Go: []byte)
	StateType_Date                    // Date (YYYY-MM-DD) (Go: time.Time)
	StateType_DateTime                // DateTime without timezone (Go: time.Time)
	StateType_DateTimeTZ              // DateTime with timezone (Go: time.Time)
	StateType_Time                    // Time without timezone (Go: time.Time)
	StateType_TimeTZ                  // Time with timezone (Go: time.Time)
	StateType_UUID                    // Universally unique identifier (Go: uuid.UUID)
	StateType_URI                     // Uniform Resource Identifier (Go: *url.URL)
)

// typeNames maps UPnP XML type names to StateVarType constants
var typeNames = map[string]StateVarType{
	"ui1":         StateType_UI1,
	"ui2":         StateType_UI2,
	"ui4":         StateType_UI4,
	"i1":          StateType_I1,
	"i2":          StateType_I2,
	"i4":          StateType_I4,
	"int":         StateType_Int,
	"r4":          StateType_R4,
	"r8":          StateType_R8,
	"number":      StateType_Number,
	"fixed.14.4":  StateType_Fixed14_4,
	"char":        StateType_Char,
	"string":      StateType_String,
	"boolean":     StateType_Boolean,
	"bin.base64":  StateType_BinBase64,
	"bin.hex":     StateType_BinHex,
	"date":        StateType_Date,
	"dateTime":    StateType_DateTime,
	"dateTime.tz": StateType_DateTimeTZ,
	"time":        StateType_Time,
	"time.tz":     StateType_TimeTZ,
	"uuid":        StateType_UUID,
	"uri":         StateType_URI,
}

// typeStrings provides string representations for StateVarType constants
var typeStrings = [...]string{
	"unknown",
	"ui1",
	"ui2",
	"ui4",
	"i1",
	"i2",
	"i4",
	"int",
	"r4",
	"r8",
	"number",
	"fixed.14.4",
	"char",
	"string",
	"boolean",
	"bin.base64",
	"bin.hex",
	"date",
	"dateTime",
	"dateTime.tz",
	"time",
	"time.tz",
	"uuid",
	"uri",
}

// String returns the UPnP XML name of the type.
// Returns "unknown" for unrecognized types.
func (t StateVarType) String() string {
	if int(t) >= 0 && int(t) < len(typeStrings) {
		return typeStrings[t]
	}
	return "unknown"
}

// ParseStateVarType converts a UPnP type name to its StateVarType constant.
// Case-insensitive and trims whitespace. Returns StateType_Unknown for unrecognized types.
func ParseStateVarType(s string) StateVarType {
	s = strings.ToLower(strings.TrimSpace(s))
	if val, ok := typeNames[s]; ok {
		return val
	}
	return StateType_Unknown
}

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
		v, ok := toUint(val, 8)
		if !ok {
			return nil, fmt.Errorf("cannot cast %v (%T) to UI1", val, val)
		}
		return uint8(v), nil

	case StateType_UI2:
		v, ok := toUint(val, 16)
		if !ok {
			return nil, fmt.Errorf("cannot cast %v (%T) to UI2", val, val)
		}
		return uint16(v), nil

	case StateType_UI4:
		v, ok := toUint(val, 32)
		if !ok {
			return nil, fmt.Errorf("cannot cast %v (%T) to UI4", val, val)
		}
		return uint32(v), nil

	case StateType_I1:
		v, ok := toInt(val, 8)
		if !ok {
			return nil, fmt.Errorf("cannot cast %v (%T) to I1", val, val)
		}
		return int8(v), nil

	case StateType_I2:
		v, ok := toInt(val, 16)
		if !ok {
			return nil, fmt.Errorf("cannot cast %v (%T) to I2", val, val)
		}
		return int16(v), nil

	case StateType_I4, StateType_Int:
		v, ok := toInt(val, 32)
		if !ok {
			return nil, fmt.Errorf("cannot cast %v (%T) to I4", val, val)
		}
		return int32(v), nil

	case StateType_R4:
		v, ok := toFloat(val, 32)
		if !ok {
			return nil, fmt.Errorf("cannot cast %v (%T) to R4", val, val)
		}
		return float32(v), nil

	case StateType_R8, StateType_Number, StateType_Fixed14_4:
		v, ok := toFloat(val, 64)
		if !ok {
			return nil, fmt.Errorf("cannot cast %v (%T) to R8", val, val)
		}
		return v, nil

	case StateType_Boolean:
		b, ok := toBool(val)
		if !ok {
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

// Cmp compares two values of the UPnP type. Returns:
//   - -1 if v1 < v2
//   - 0 if v1 == v2
//   - 1 if v1 > v2
//
// Panics if values can't be cast to the type. Handles all supported types
// including binaries, times, and UUIDs.
func (t StateVarType) Cmp(v1, v2 interface{}) int {
	// Helper: compare float64
	compareFloat := func(f1, f2 float64) int {
		switch {
		case f1 < f2:
			return -1
		case f1 > f2:
			return 1
		default:
			return 0
		}
	}

	castV1, err1 := t.Cast(v1)
	castV2, err2 := t.Cast(v2)

	if err1 != nil || err2 != nil {
		log.Fatalf("Failed to cast for comparison: %v vs %v (errors: %v, %v)", v1, v2, err1, err2)
	}

	switch t {
	case StateType_UI1, StateType_UI2, StateType_UI4,
		StateType_Int, StateType_I1, StateType_I2, StateType_I4:
		i1 := reflect.ValueOf(castV1).Int()
		i2 := reflect.ValueOf(castV2).Int()

		switch {
		case i1 < i2:
			return -1
		case i1 > i2:
			return 1
		default:
			return 0
		}

	case StateType_R4:
		f1 := float64(castV1.(float32))
		f2 := float64(castV2.(float32))
		return compareFloat(f1, f2)

	case StateType_R8, StateType_Number, StateType_Fixed14_4:
		f1 := castV1.(float64)
		f2 := castV2.(float64)
		return compareFloat(f1, f2)

	case StateType_Boolean:
		b1 := castV1.(bool)
		b2 := castV2.(bool)
		switch {
		case b1 == b2:
			return 0
		case !b1 && b2:
			return -1
		default:
			return 1
		}

	case StateType_Char:
		r1 := castV1.(rune)
		r2 := castV2.(rune)

		switch {
		case r1 < r2:
			return -1
		case r1 > r2:
			return 1
		default:
			return 0
		}

	case StateType_String, StateType_UUID, StateType_URI:
		s1 := castV1.(string)
		s2 := castV2.(string)
		return strings.Compare(s1, s2)

	case StateType_BinBase64, StateType_BinHex:
		b1 := castV1.([]byte)
		b2 := castV2.([]byte)
		return bytes.Compare(b1, b2)

	case StateType_Date, StateType_DateTime, StateType_DateTimeTZ,
		StateType_Time, StateType_TimeTZ:
		t1 := castV1.(time.Time)
		t2 := castV2.(time.Time)
		if t1.Before(t2) {
			return -1
		} else if t1.After(t2) {
			return 1
		}
		return 0

	default:
		s1 := fmt.Sprint(castV1)
		s2 := fmt.Sprint(castV2)
		return strings.Compare(s1, s2)
	}
}

// InRange checks if a value falls within an inclusive range [min, max].
// Uses the type's comparison logic. Returns true if val is between min and max (inclusive).
//
// Example:
//
//	range := ValueRange{min: uint16(10), max: uint16(100)}
//	StateType_UI2.InRange(uint16(50), range) // true
func (t StateVarType) InRange(val interface{}, interval *ValueRange) bool {
	return interval == nil || t.Cmp(val, interval.min) >= 0 && t.Cmp(val, interval.max) <= 0
}

// ValueRange creates a valid value range for the UPnP type.
//
// This method casts the provided min and max values to the UPnP type and returns
// a ValueRange struct suitable for range validation. If either value cannot be
// cast to the type, it returns an error.
//
// Parameters:
//
//	min: Minimum value of the range (inclusive)
//	max: Maximum value of the range (inclusive)
//
// Returns:
//
//	ValueRange: Valid range structure if cast succeeds
//	error: If min or max cannot be cast to the type
//
// Example:
//
//	// Create a range for UI2 (uint16) values
//	r, err := StateType_UI2.ValueRange(10, 100)
//	if err != nil { /* handle error */ }
//	valid := StateType_UI2.InRange(50, r) // true
//
// Notes:
//   - The range is inclusive: [min, max]
//   - Values must be comparable using the type's comparison logic
//   - For time types, min and max must be valid time values
func (t StateVarType) ValueRange(min, max interface{}) (*ValueRange, error) {
	cmin, error := t.Cast(min)
	if error != nil {
		return nil, fmt.Errorf("min value %v is not castable to type %s", min, t.String())
	}
	cmax, error := t.Cast(max)
	if error != nil {
		return nil, fmt.Errorf("max value %v is not castable to type %s", min, t.String())
	}

	return &ValueRange{min: cmin, max: cmax}, nil
}

// NewAtomicValue crée une valeur simple
func (t StateVarType) NewAtomicValue(name string) *StateValue {
	return &StateValue{
		name:     name,
		baseType: t,
		modifier: ModifierAtomic,
	}
}

// NewListValue crée une liste
func (t StateVarType) NewListValue(name string, elementType StateVarType) *StateValue {
	return &StateValue{
		name:        name,
		baseType:    t,
		modifier:    ModifierList,
		elementType: elementType,
	}
}

// NewMapValue crée une map
func (t StateVarType) NewMapValue(
	name string,
	keyType StateVarType,
	valueType StateVarType,
) *StateValue {
	return &StateValue{
		name:        name,
		baseType:    t,
		modifier:    ModifierMap,
		keyType:     keyType,
		elementType: valueType, // elementType = valeur de la map
	}
}

// NewStructValue crée une valeur de type struct
func (t StateVarType) NewStructValue(name string, fields map[string]StateVarType) *StateValue {
	return &StateValue{
		name:         name,
		baseType:     t,
		modifier:     ModifierStruct,
		structFields: fields,
	}
}

func (t StateVarType) DefaultValue() interface{} {
	switch t {
	case StateType_Unknown:

	case StateType_UI1, StateType_UI2, StateType_UI4:
		return uint64(0)

	case StateType_I1, StateType_I2, StateType_I4, StateType_Int:
		return int64(0)

	case StateType_R4, StateType_R8, StateType_Number, StateType_Fixed14_4:
		return float64(0)

	case StateType_Char, StateType_String:
		return ""
	case StateType_Boolean:
		return false

	case StateType_BinBase64:
		return ""
	case StateType_BinHex:
		return ""
	case StateType_Date:
		return time.Unix(int64(1718985600), 0).UTC()
	case StateType_DateTime:
		return time.Unix(int64(1718985600), 0).UTC()
	case StateType_DateTimeTZ:
		return time.Unix(int64(1718985600), 0).UTC()
	case StateType_Time:
		return time.Unix(int64(1718985600), 0).UTC()
	case StateType_TimeTZ:
		return time.Unix(int64(1718985600), 0).UTC()
	case StateType_UUID:
		return ""
	case StateType_URI:
		return ""

	}

	return nil
}

// toInt converts various types to signed integer with specified bit size.
// Handles overflow/underflow. Returns converted value and success status.
func toInt(v interface{}, bits int) (int64, bool) {
	min := minInt(bits)
	max := maxInt(bits)

	switch val := v.(type) {
	case int:
		if int64(val) < min || int64(val) > max {
			return 0, false
		}
		return int64(val), true
	case int8:
		if int64(val) < min || int64(val) > max {
			return 0, false
		}
		return int64(val), true
	case int16:
		if int64(val) < min || int64(val) > max {
			return 0, false
		}
		return int64(val), true
	case int32:
		if int64(val) < min || int64(val) > max {
			return 0, false
		}
		return int64(val), true
	case int64:
		if val < min || val > max {
			return 0, false
		}
		return val, true
	case uint:
		if uint64(val) > uint64(max) {
			return 0, false
		}
		return int64(val), true
	case uint8:
		if uint64(val) > uint64(max) {
			return 0, false
		}
		return int64(val), true
	case uint16:
		if uint64(val) > uint64(max) {
			return 0, false
		}
		return int64(val), true
	case uint32:
		if uint64(val) > uint64(max) {
			return 0, false
		}
		return int64(val), true
	case uint64:
		if val > uint64(max) {
			return 0, false
		}
		return int64(val), true
	case float32:
		r := int64(math.Round(float64(val)))
		if r < min || r > max {
			return 0, false
		}
		return r, true
	case float64:
		r := int64(math.Round(val))
		if r < min || r > max {
			return 0, false
		}
		return r, true
	case string:
		// Try int parse direct
		if i, err := strconv.ParseInt(val, 10, bits); err == nil {
			if i < min || i > max {
				return 0, false
			}
			return i, true
		}
		// Try float parse then round + bounds check
		if f, err := strconv.ParseFloat(val, 64); err == nil {
			r := int64(math.Round(f))
			if r < min || r > max {
				return 0, false
			}
			return r, true
		}
		return 0, false
	default:
		return 0, false
	}
}

// toUint converts various types to unsigned integer with specified bit size.
// Handles numeric types and strings. Returns converted value and success status.
func toUint(v interface{}, bits int) (uint64, bool) {
	max := maxUint(bits)

	switch val := v.(type) {
	case uint:
		if uint64(val) > max {
			return 0, false
		}
		return uint64(val), true
	case uint8:
		if uint64(val) > max {
			return 0, false
		}
		return uint64(val), true
	case uint16:
		if uint64(val) > max {
			return 0, false
		}
		return uint64(val), true
	case uint32:
		if uint64(val) > max {
			return 0, false
		}
		return uint64(val), true
	case uint64:
		if val > max {
			return 0, false
		}
		return val, true

	case int:
		if val < 0 || uint64(val) > max {
			return 0, false
		}
		return uint64(val), true
	case int8:
		if val < 0 || uint64(val) > max {
			return 0, false
		}
		return uint64(val), true
	case int16:
		if val < 0 || uint64(val) > max {
			return 0, false
		}
		return uint64(val), true
	case int32:
		if val < 0 || uint64(val) > max {
			return 0, false
		}
		return uint64(val), true
	case int64:
		if val < 0 || uint64(val) > max {
			return 0, false
		}
		return uint64(val), true

	case float32:
		r := uint64(math.Round(float64(val)))
		if r > max {
			return 0, false
		}
		return r, true
	case float64:
		r := uint64(math.Round(val))
		if r > max {
			return 0, false
		}
		return r, true

	case string:
		// Try parse float first (handles int and float strings)
		f, err := strconv.ParseFloat(val, 64)
		if err != nil || f < 0 {
			return 0, false
		}
		r := uint64(math.Round(f))
		if r > max {
			return 0, false
		}
		return r, true

	default:
		return 0, false
	}
}

// maxUint returns maximum unsigned integer value for specified bit size
func maxUint(bits int) uint64 {
	switch bits {
	case 8:
		return math.MaxUint8
	case 16:
		return math.MaxUint16
	case 32:
		return math.MaxUint32
	case 64:
		return math.MaxUint64
	default:
		return math.MaxUint64 // fallback
	}
}

// minInt returns minimum signed integer value for specified bit size
func minInt(bits int) int64 {
	switch bits {
	case 8:
		return math.MinInt8
	case 16:
		return math.MinInt16
	case 32:
		return math.MinInt32
	case 64:
		return math.MinInt64
	default:
		return math.MinInt64 // fallback
	}
}

// maxInt returns maximum signed integer value for specified bit size
func maxInt(bits int) int64 {
	switch bits {
	case 8:
		return math.MaxInt8
	case 16:
		return math.MaxInt16
	case 32:
		return math.MaxInt32
	case 64:
		return math.MaxInt64
	default:
		return math.MaxInt64 // fallback
	}
}

// toFloat converts various types to float (32 or 64 bits).
// Checks float32 boundaries when converting to 32-bit float.
func toFloat(v interface{}, bits int) (float64, bool) {
	switch val := v.(type) {
	case float32:
		f := float64(val)
		if bits == 32 && (f > math.MaxFloat32 || f < -math.MaxFloat32) {
			return 0, false
		}
		return f, true
	case float64:
		if bits == 32 && (val > math.MaxFloat32 || val < -math.MaxFloat32) {
			return 0, false
		}
		return val, true
	case int, int8, int16, int32, int64:
		f := float64(reflect.ValueOf(val).Int())
		if bits == 32 && (f > math.MaxFloat32 || f < -math.MaxFloat32) {
			return 0, false
		}
		return f, true
	case uint, uint8, uint16, uint32, uint64:
		f := float64(reflect.ValueOf(val).Uint())
		if bits == 32 && (f > math.MaxFloat32 || f < -math.MaxFloat32) {
			return 0, false
		}
		return f, true
	case string:
		f, err := strconv.ParseFloat(val, bits)
		if err != nil {
			return 0, false
		}
		if bits == 32 && (f > math.MaxFloat32 || f < -math.MaxFloat32) {
			return 0, false
		}
		return f, true
	default:
		return 0, false
	}
}

// toBool converts various types to boolean following UPnP rules:
// true: 1, "true"; false: 0, "false"
func toBool(val interface{}) (bool, bool) {
	switch v := val.(type) {
	case bool:
		return v, true

	case int:
		if v == 0 {
			return false, true
		} else if v == 1 {
			return true, true
		}
	case int8:
		if v == 0 {
			return false, true
		} else if v == 1 {
			return true, true
		}
	case int16:
		if v == 0 {
			return false, true
		} else if v == 1 {
			return true, true
		}
	case int32:
		if v == 0 {
			return false, true
		} else if v == 1 {
			return true, true
		}
	case int64:
		if v == 0 {
			return false, true
		} else if v == 1 {
			return true, true
		}

	case uint:
		if v == 0 {
			return false, true
		} else if v == 1 {
			return true, true
		}
	case uint8:
		if v == 0 {
			return false, true
		} else if v == 1 {
			return true, true
		}
	case uint16:
		if v == 0 {
			return false, true
		} else if v == 1 {
			return true, true
		}
	case uint32:
		if v == 0 {
			return false, true
		} else if v == 1 {
			return true, true
		}
	case uint64:
		if v == 0 {
			return false, true
		} else if v == 1 {
			return true, true
		}

	case float32:
		if v == 0.0 {
			return false, true
		} else if v == 1.0 {
			return true, true
		}
	case float64:
		if v == 0.0 {
			return false, true
		} else if v == 1.0 {
			return true, true
		}

	case string:
		return parseUPnPBoolean(v)
	}

	return false, false
}

// parseUPnPBoolean parses boolean from string:
// "1"/"true" → true, "0"/"false" → false
func parseUPnPBoolean(s string) (bool, bool) {
	switch strings.TrimSpace(strings.ToLower(s)) {
	case "1", "true":
		return true, true
	case "0", "false":
		return false, true
	default:
		return false, false
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

// parseUPnPTime parses time values using UPnP-specific formats:
//   - Date: "2006-01-02"
//   - Time: "15:04:05"
//   - DateTime: "2006-01-02T15:04:05"
//   - TimeTZ: "15:04:05-07:00"
//   - DateTimeTZ: "2006-01-02T15:04:05-07:00"
func parseUPnPTime(t StateVarType, s string) (time.Time, error) {
	s = strings.TrimSpace(s)

	layouts := []string{}

	switch t {
	case StateType_Date:
		layouts = []string{"2006-01-02"}

	case StateType_Time:
		layouts = []string{"15:04:05"} // HH:MM:SS

	case StateType_TimeTZ:
		layouts = []string{"15:04:05Z07:00"} // HH:MM:SS+TZ

	case StateType_DateTime:
		layouts = []string{"2006-01-02T15:04:05"} // ISO8601 sans TZ

	case StateType_DateTimeTZ:
		layouts = []string{
			"2006-01-02T15:04:05Z07:00", // full
			"2006-01-02T15:04:05-0700",  // fallback no colon
			"2006-01-02T15:04:05Z",      // Zulu
		}

	default:
		return time.Time{}, fmt.Errorf("unsupported date/time type: %v", t)
	}

	for _, layout := range layouts {
		if ts, err := time.Parse(layout, s); err == nil {
			return ts, nil
		}
	}
	return time.Time{}, fmt.Errorf("invalid %v value: %q", t, s)
}
