package statevariables

import (
	"bytes"
	"fmt"
	"log"
	"strings"
)

// Cmp compares two values of a given type 'StateVarType'. The comparison is
// made based on the specific type of 'a' and 'b'. It returns an integer
// indicating whether 'a' is less than, equal to, or greater than 'b'. If 'a' is
// less than 'b', it returns -1. If they are equal, it returns 0. If 'a' is
// greater than 'b', it returns 1. It also returns an error if any of the values
// can't be cast to the required type or if comparison isn't supported for the
// given type 'StateVarType'.
//
//   - t: StateVarType representing the specific type to use for comparison.
//   - a, b: The values to compare. They should be of type interface{} as they could be
//     any valid Go type.
//
// Returns: An integer indicating the result of the comparison (-1 if 'a' is
// less than 'b', 0 if they are equal, and 1 if 'a' is greater than 'b'). It
// also returns an error if any values can't be cast or if comparison isn't
// supported for the given type.
//
// Example:
//
//	result, err := t.Cmp(int32(5), int32(7)) // Returns -1 and nil error as 5 is less than 7.
func (t StateVarType) Cmp(a, b interface{}) (int, error) {
	a, err1 := t.Cast(a)
	b, err2 := t.Cast(b)

	if err1 != nil || err2 != nil {
		log.Fatalf("Failed to cast for comparison: %v vs %v (errors: %v, %v)", a, b, err1, err2)
	}

	switch {
	case t.IsInteger():
		ai, err := toInt(a, t.BitSize())
		if err != nil {
			return 0, fmt.Errorf("invalid int value for a: %w", err)
		}
		bi, err := toInt(b, t.BitSize())
		if err != nil {
			return 0, fmt.Errorf("invalid int value for b: %w", err)
		}
		return cmpInt(ai, bi), nil

	case t.IsUnsignedInt():
		ai, err := toUint(a, t.BitSize())
		if err != nil {
			return 0, fmt.Errorf("invalid uint value for a: %w", err)
		}
		bi, err := toUint(b, t.BitSize())
		if err != nil {
			return 0, fmt.Errorf("invalid uint value for b: %w", err)
		}
		return cmpUint(ai, bi), nil

	case t.IsFloat():
		af, err := toFloat(a, t.BitSize())
		if err != nil {
			return 0, fmt.Errorf("invalid float value for a: %w", err)
		}
		bf, err := toFloat(b, t.BitSize())
		if err != nil {
			return 0, fmt.Errorf("invalid float value for b: %w", err)
		}
		return cmpFloat64(af, bf), nil

	case t == StateType_Boolean:
		ab, err := toBool(a)
		if err != nil {
			return 0, fmt.Errorf("invalid bool value for a: %w", err)
		}
		bb, err := toBool(b)
		if err != nil {
			return 0, fmt.Errorf("invalid bool value for b: %w", err)
		}
		return cmpBool(ab, bb), nil

	case t == StateType_String || t == StateType_Char:
		as, err := toString(a)
		if err != nil {
			return 0, fmt.Errorf("invalid string value for a")
		}
		bs, err := toString(b)
		if err != nil {
			return 0, fmt.Errorf("invalid string value for b")
		}
		return strings.Compare(as, bs), nil

	case t.IsTime():
		at, err := toTime(a)
		if err != nil {
			return 0, fmt.Errorf("invalid time value for a")
		}
		bt, err := toTime(b)
		if err != nil {
			return 0, fmt.Errorf("invalid time value for b")
		}
		return cmpTime(at, bt), nil

	default:
		return 0, fmt.Errorf("comparison not supported for type %v", t)
	}
}

func (t StateVarType) Equal(a, b interface{}) (bool, error) {
	switch {
	case t.IsInteger():
		ai, err1 := toInt(a, 64)
		bi, err2 := toInt(b, 64)
		if err1 != nil || err2 != nil {
			return false, fmt.Errorf("invalid integer value for type %s", t.String())
		}
		return ai == bi, nil

	case t.IsFloat():
		af, err := toFloat(a, 64)
		if err != nil {
			return false, fmt.Errorf("invalid float value for type %s: %v", t.String(), err)
		}
		bf, err := toFloat(b, 64)
		if err != nil {
			return false, fmt.Errorf("invalid float value for type %s: %v", t.String(), err)
		}
		return af == bf, nil

	case t.IsString():
		as, ok1 := a.(string)
		bs, ok2 := b.(string)
		if !ok1 || !ok2 {
			return false, fmt.Errorf("invalid string value for type %s", t.String())
		}
		return as == bs, nil

	case t.IsBool():
		ab, err1 := toBool(a)
		bb, err2 := toBool(b)
		if err1 != nil || err2 != nil {
			return false, fmt.Errorf("invalid boolean value for type %s", t.String())
		}
		return ab == bb, nil

	case t.IsTime():
		at, err1 := toTime(a)
		bt, err2 := toTime(b)
		if err1 != nil || err2 != nil {
			return false, fmt.Errorf("invalid time.Time value for type %s", t.String())
		}
		return at.Equal(bt), nil

	case t.IsUUID():
		au, err1 := toUUID(a)
		bu, err2 := toUUID(b)
		if err1 != nil || err2 != nil {
			return false, fmt.Errorf("invalid uuid.UUID value for type %s", t.String())
		}
		return au == bu, nil

	case t.IsURI():
		au, err1 := toURI(a)
		bu, err2 := toURI(b)
		if err1 != nil || err2 != nil {
			return false, fmt.Errorf("invalid *url.URL value for type %s", t.String())
		}
		return au.String() == bu.String(), nil

	case t.IsBinary():
		ab, err1 := toBinary(a)
		bb, err2 := toBinary(b)
		if err1 != nil || err2 != nil {
			return false, fmt.Errorf("invalid []byte value for type %s", t.String())
		}
		return bytes.Equal(ab, bb), nil

	default:
		return false, fmt.Errorf("equality not supported for type %s", t.String())
	}
}

// InRange checks if a value falls within an inclusive range [min, max].
// Uses the type's comparison logic. Returns true if val is between min and max (inclusive).
//
// Example:
//
//	range := ValueRange{min: uint16(10), max: uint16(100)}
//	StateType_UI2.InRange(uint16(50), range) // true
func (t StateVarType) InRange(val interface{}, interval *ValueRange) (bool, error) {
	if interval == nil {
		return true, nil
	}
	cmp1, err1 := t.Cmp(val, interval.min)
	cmp2, err2 := t.Cmp(val, interval.max)
	if err1 != nil || err2 != nil {
		err := err1
		if err == nil {
			err = err2
		}
		return false, err
	}
	return cmp1 >= 0 && cmp2 <= 0, nil
}
