// package stateVariables provides comprehensive handling of UPnP state variable types.
// It includes type identification, value casting, comparison, and range validation
// for all standard UPnP state variable types.
package statevariables

import (
	"strings"
	"time"
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

// StateVarTypeFactory takes a string and attempts to build from it a valid
// StateVarType. The input string is cleaned before processing -
// leading/trailing spaces are trimmed, the case is lowered for comparison with
// known types, and if no match is found 'StateType_Unknown' is returned.
func StateVarTypeFactory(s string) StateVarType {
	s = strings.ToLower(strings.TrimSpace(s))
	if val, ok := typeNames[s]; ok {
		return val
	}
	return StateType_Unknown
}

// String returns a string representation of the StateVarType. It defaults to
// "unknown" if the type is not recognized.
func (t StateVarType) String() string {
	if int(t) >= 0 && int(t) < len(typeStrings) {
		return typeStrings[t]
	}
	return "unknown"
}

// BitSize returns the bit size of the StateVarType value, or -1 if not numeric.
// The possible return values are 8 for StateTypes I1, UI1, and 64 for others.
func (t StateVarType) BitSize() int {
	// If t isn't numeric, return -1
	if !t.IsNumeric() {
		return -1
	}

	// Check the value of t and return the appropriate bit size
	switch t {
	case StateType_I1, StateType_UI1:
		return 8
	case StateType_I2, StateType_UI2:
		return 16
	case StateType_I4, StateType_UI4, StateType_Int, StateType_R4:
		return 32
	case StateType_R8, StateType_Number, StateType_Fixed14_4:
		return 64
	default:
		return 64
	}
}

// NewStateValue creates and returns a new StateValue struct instance with the given name
// and the receiver's state variable type. The created StateValue is initialized with an
// empty map for event conditions. If name is an empty string, it will cause panic in later
// usage. Name is typically used to identify or represent a specific value or condition
// associated with the variable type 't'.
func (t StateVarType) NewStateValue(name string) *StateVariable {
	return &StateVariable{
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
