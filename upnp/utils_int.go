package upnp

import (
	"errors"
	"math"
	"reflect"
	"strconv"
)

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

// toInt converts the given interface value into an int64. The function accepts
// a parameter v of any type and an integer bits representing the size of the
// desired integer type (8, 16, 32 or 64). If successful, it returns the
// converted integer and nil for error. Otherwise, it returns zero for int64 and
// an appropriate error message.
//
// The function checks whether v is nil. If true, it returns an error stating
// "cannot convert nil to int".
//
// It then identifies the type of v using a type switch statement. Depending on
// the type, the function performs different actions:
//   - For integer types (int, int8, int16, int32 and int64), it uses checkIntBounds() to ensure the value fits within the specified bits range and returns the result.
//   - For unsigned types (uint, uint8, uint16, uint32 and uint64), it checks for overflow before converting to an int64 and calls checkIntBounds().
//   - For float types (float32 and float64), it converts them to int64 directly.
//   - For string type, it attempts to parse the string as a base-10 integer using strconv.ParseInt() with specified bits range.
//
// If no match is found in these cases or v is of unsupported type, it returns
// an error stating "unsupported type for toInt".
func toInt(v interface{}, bits int) (int64, error) {
	if v == nil {
		return 0, errors.New("cannot convert nil to int")
	}

	switch val := v.(type) {
	case int:
		return checkIntBounds(int64(val), bits)
	case int8:
		return checkIntBounds(int64(val), bits)
	case int16:
		return checkIntBounds(int64(val), bits)
	case int32:
		return checkIntBounds(int64(val), bits)
	case int64:
		return checkIntBounds(val, bits)

	case uint, uint8, uint16, uint32, uint64:
		u := reflect.ValueOf(val).Uint()
		if u > uint64(math.MaxInt64) {
			return 0, errors.New("unsigned value overflows int64")
		}
		return checkIntBounds(int64(u), bits)

	case float32:
		return checkIntBounds(int64(val), bits)
	case float64:
		return checkIntBounds(int64(val), bits)

	case string:
		i, err := strconv.ParseInt(val, 10, bits)
		if err != nil {
			return 0, err
		}
		return checkIntBounds(i, bits)

	default:
		return 0, errors.New("unsupported type for toInt")
	}
}

// CheckIntBounds checks if a given int64 value is within the valid range for a specific number of bits.
// It returns an error and 0 if the value is out of bounds, else it returns the input value and nil.
//
// Parameters:
// - v: The integer value to check.
// - bits: The number of bits that determine the valid range for 'v'.
//
// Returns:
// - int64: The original input value if within bounds, else 0.
// - error: An error object if the value is out of bounds; nil otherwise.
func checkIntBounds(v int64, bits int) (int64, error) {
	if v < minInt(bits) || v > maxInt(bits) {
		return 0, errors.New("integer value out of bounds")
	}
	return v, nil
}
