package upnp

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
