package statevariables

import (
	"errors"
	"math"
	"reflect"
	"strconv"
)

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

// toUint converts various types of numeric values into a uint64 type. It
// supports conversions from signed and unsigned integers, floating-point
// numbers, and string representations of integers.
//
// Parameters:
//   - v interface{}: input value that can be converted to uint64. The
//     function will return an error if the input is not one of these types.
//   - bits int: number of bits that should fit within the returned uint64.
//     An error will be returned if the conversion would exceed this limit.
//
// Returns:
//   - uint64: converted value from input 'v'. If the input 'v' is a string,
//     it must represent an integer in base 10 and can fit into an uint64
//     type with given number of bits.
//   - error: if the conversion fails due to unsupported input type, overflow
//     or underflow conditions, or invalid string representation, this will
//     contain an appropriate error message. If 'v' is nil,
//     it returns an "cannot convert nil to uint" error.
func toUint(v interface{}, bits int) (uint64, error) {
	if v == nil {
		return 0, errors.New("cannot convert nil to uint")
	}

	switch val := v.(type) {
	case uint:
		return checkUintBounds(uint64(val), bits)
	case uint8:
		return checkUintBounds(uint64(val), bits)
	case uint16:
		return checkUintBounds(uint64(val), bits)
	case uint32:
		return checkUintBounds(uint64(val), bits)
	case uint64:
		return checkUintBounds(val, bits)

	case int, int8, int16, int32, int64:
		i := reflect.ValueOf(val).Int()
		if i < 0 {
			return 0, errors.New("negative value cannot be converted to uint")
		}
		return checkUintBounds(uint64(i), bits)

	case float32:
		if val < 0 {
			return 0, errors.New("negative float cannot be converted to uint")
		}
		return checkUintBounds(uint64(val), bits)
	case float64:
		if val < 0 {
			return 0, errors.New("negative float cannot be converted to uint")
		}
		return checkUintBounds(uint64(val), bits)

	case string:
		u, err := strconv.ParseUint(val, 10, bits)
		if err != nil {
			return 0, err
		}
		return checkUintBounds(u, bits)

	default:
		return 0, errors.New("unsupported type for toUint")
	}
}

// checkUintBounds checks whether the given unsigned integer 'v' is within the
// acceptable range defined by the number of bits 'bits'. If 'v' is out of
// bounds, it returns an error with a message "unsigned integer value out of
// bounds". Otherwise, it returns 'v' and nil.
func checkUintBounds(v uint64, bits int) (uint64, error) {
	if v > maxUint(bits) {
		return 0, errors.New("unsigned integer value out of bounds")
	}
	return v, nil
}
