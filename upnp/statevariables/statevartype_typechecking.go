package statevariables

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

// IsSignedInt checks if the state variable type is a signed integer type.
// The function returns true for StateType_I1, StateType_I2, StateType_I4, and
// StateType_Int, otherwise it will return false. This method is part of the
// StateVarType enumeration in statevaluetype package. It takes no parameters
// but operates on the receiver 't' of type StateVarType.
//
// The returned value is a boolean.
func (t StateVarType) IsSignedInt() bool {
	switch t {
	case StateType_I1, StateType_I2, StateType_I4, StateType_Int:
		return true
	default:
		return false
	}
}

// IsUnsignedInt checks if the state variable type is an unsigned integer. It
// returns a boolean indicating whether or not the current StateVarType
// represents an unsigned integer type, namely: StateType_UI1, StateType_UI2,
// and StateType_UI4.
func (t StateVarType) IsUnsignedInt() bool {
	switch t {
	case StateType_UI1, StateType_UI2, StateType_UI4:
		return true
	default:
		return false
	}
}

// IsFloat returns a boolean indicating whether the given state variable type
// represents a float number. If the state variable type is one of R4, R8, Number or
// Fixed14_4 it returns true; otherwise, it returns false.
func (t StateVarType) IsFloat() bool {
	switch t {
	case StateType_R4, StateType_R8, StateType_Number, StateType_Fixed14_4:
		return true
	default:
		return false
	}
}

// IsBool checks if a StateVarType is of type Boolean. It returns true if the
// StateVarType equals to StateType_Boolean, false otherwise.
func (t StateVarType) IsBool() bool {
	return t == StateType_Boolean
}

// IsString reports whether or not the state variable type represents a string
// value.
//
// This method returns true if the StateVarType is either StateType_String or
// StateType_Char, otherwise it returns false.
func (t StateVarType) IsString() bool {
	switch t {
	case StateType_String, StateType_Char:
		return true
	default:
		return false
	}
}

// IsTime checks whether a given StateVarType is of time type or not. It accepts
// a StateVarType parameter 't' and returns a boolean value based on the check.
//
// The possible values for 't' are: StateType_Date, StateType_DateTime,
// StateType_DateTimeTZ, StateType_Time, StateType_TimeTZ. If 't' is any of
// these types, the function returns true; otherwise, it returns false.
func (t StateVarType) IsTime() bool {
	switch t {
	case StateType_Date, StateType_DateTime, StateType_DateTimeTZ,
		StateType_Time, StateType_TimeTZ:
		return true
	default:
		return false
	}
}

// IsUUID reports whether the receiver represents a UUID (Universally Unique
// Identifier). The StateVarType should be of type StateType_UUID to return
// true. Otherwise, it returns false.
func (t StateVarType) IsUUID() bool {
	return t == StateType_UUID
}

// IsURI checks if the given state variable type is a URI.
//
// This function returns true if and only if the receiver (StateVarType t)
// equals StateType_URI, which represents URIs in UPnP protocol. Otherwise, it
// returns false.
func (t StateVarType) IsURI() bool {
	return t == StateType_URI
}

// IsBinary checks if the given StateVarType is binary type or not. It returns
// true for types StateType_BinBase64 and StateType_BinHex, otherwise it returns
// false.
func (t StateVarType) IsBinary() bool {
	switch t {
	case StateType_BinBase64, StateType_BinHex:
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
