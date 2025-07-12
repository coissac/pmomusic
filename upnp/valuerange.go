package upnp

import "fmt"

// ValueRange represents an inclusive range constraint for a state variable value.
// It defines the minimum and maximum allowable values for a given UPnP type.
//
// Usage:
//   - For numeric types: min/max must be numeric types
//   - For time types: min/max must be time.Time values
//   - For strings/UUIDs: min/max must be string values
//
// Use with StateVarType.InRange() to check if values fall within the range.
type ValueRange struct {
	min interface{}
	max interface{}
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

	if t.Cmp(cmin, cmax) > 0 {
		cmax, cmin = cmin, cmax
	}

	return &ValueRange{min: cmin, max: cmax}, nil
}
