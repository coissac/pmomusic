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
	"net/url"
	"reflect"
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

// String returns a string representation of the StateVarType. It defaults to
// "unknown" if the type is not recognized.
func (t StateVarType) String() string {
	if int(t) >= 0 && int(t) < len(typeStrings) {
		return typeStrings[t]
	}
	return "unknown"
}

// IsNumeric checks whether a given StateVarType represents a numeric type or
// not. Numeric types are defined as those that can be used to store number-like
// values. The following types are considered numeric: UI1, UI2, UI4, I1, I2,
// I4, Int, R4, R8, Number and Fixed14_4.
//
// t: StateVarType to check if it's a numeric type or not.
//
// Returns true if the given StateVarType represents a numeric type; false
// otherwise.
func (t StateVarType) IsNumeric() bool {
	switch t {
	case StateType_UI1, StateType_UI2, StateType_UI4,
		StateType_I1, StateType_I2, StateType_I4,
		StateType_Int,
		StateType_R4, StateType_R8,
		StateType_Number,
		StateType_Fixed14_4:
		return true
	default:
		return false
	}
}

// IsInteger checks if the state variable type is integer or not.
//
// It returns a boolean value indicating whether the provided StateVarType (t)
// is an integer type or not. The function takes one parameter, t of type
// StateVarType, which represents the state variable type to be checked.
//
// Parameters: - t (StateVarType): The StateVarType to check for comparability.
//
// Returns: bool: If the state variable type is any of the defined integer types
// (StateType_UI1, StateType_UI2, StateType_UI4, StateType_I1, StateType_I2,
// StateType_I4, StateType_Int), it returns true. Otherwise, it returns false.
func (t StateVarType) IsInteger() bool {
	switch t {
	case StateType_UI1, StateType_UI2, StateType_UI4,
		StateType_I1, StateType_I2, StateType_I4,
		StateType_Int:
		return true
	default:
		return false
	}
}

// IsComparable function checks if a StateVarType is comparable or not.
//
// It returns false for binary types (StateType_BinBase64 and StateType_BinHex)
// as they are non-comparable. For all other types, it returns true indicating
// that these types can be compared.
//
// Parameters: - t (StateVarType): The StateVarType to check for comparability.
//
// Returns: bool: A boolean value indicating whether the given StateVarType is
// comparable or not. True means it's comparable, False means it isn't.
func (t StateVarType) IsComparable() bool {
	// Tous les types sauf les binaires sont comparables
	switch t {
	case StateType_BinBase64, StateType_BinHex:
		return false
	default:
		return true
	}
}

// Add performs addition operation on two interfaces if both are of numeric
// type, otherwise it returns an error. If the types are not numeric, it checks
// and converts them into float64 before performing the addition. The function
// then casts the result back to its original type using Cast method from
// StateVarType t and returns this value or any encountered error.
//
// Parameters:
//
//	a (interface{}): First operand for addition operation. Can be of any type.
//	b (interface{}): Second operand for addition operation. Can be of any type.
//
// Returns:
//
//	interface{}: Result of the addition, casted back to its original type using StateVarType t if no error encountered.
//	error: Encountered error in case any conversion or casting fails. This includes non-numeric types for this operation.
func (t StateVarType) Add(a, b interface{}) (interface{}, error) {
	af, bf, err := valuesToNumericOperands(t, a, b)
	if err != nil {
		return nil, err
	}

	return t.Cast(af + bf)
}

func (t StateVarType) Sub(a, b interface{}) (interface{}, error) {
	af, bf, err := valuesToNumericOperands(t, a, b)
	if err != nil {
		return nil, err
	}

	return t.Cast(af - bf)
}

func (t StateVarType) Mul(a, b interface{}) (interface{}, error) {
	af, bf, err := valuesToNumericOperands(t, a, b)
	if err != nil {
		return nil, err
	}

	return t.Cast(af * bf)
}

func (t StateVarType) Div(a, b interface{}) (interface{}, error) {
	af, bf, err := valuesToNumericOperands(t, a, b)
	if err != nil {
		return nil, err
	}

	return t.Cast(af / bf)
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

// NewAtomicValue crée une valeur simple
func (t StateVarType) NewAtomicValue(name string) *StateValue {
	return &StateValue{
		name:            name,
		valueType:       t,
		eventConditions: make(map[string]StateConditionFunc),
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
