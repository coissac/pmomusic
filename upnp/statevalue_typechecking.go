package upnp

// IsNumeric checks whether a given StateValue model represents a numeric value or
// not. Numeric types are defined as those that can be used to store number-like
// values. The following types are considered numeric: UI1, UI2, UI4, I1, I2,
// I4, Int, R4, R8, Number and Fixed14_4.
//
// t: StateVarType to check if it's a numeric type or not.
//
// Returns true if the given StateValue represents a numeric value; false
// otherwise.
func (sv StateValue) IsNumeric() bool {
	return sv.valueType.IsNumeric()
}

// IsInteger checks if the state variable type is integer or not.
//
// It returns a boolean value indicating whether the provided StateValue
// is an integer type or not.
//
// Returns: bool: If the state variable type is any of the defined integer types
// (UI1, UI2, UI4, I1, I2, I4, Int), it returns true. Otherwise, it returns false.
func (sv StateValue) IsInteger() bool {
	return sv.valueType.IsInteger()
}

// IsSignedInt checks if the state value type is a signed int.
//
// The return value will be a boolean indicating whether the state value type is
// a signed integer (true) or not (false).
func (sv StateValue) IsSignedInt() bool {
	return sv.valueType.IsSignedInt()
}

// IsUnsignedInt checks if the state value type represents an unsigned integer.
// The method returns a boolean indicating whether the state value type is an
// unsigned integer.
func (sv StateValue) IsUnsignedInt() bool {
	return sv.valueType.IsUnsignedInt()
}

// IsFloat returns true if the StateValue's value type represents a floating point
// number; false otherwise.
func (sv StateValue) IsFloat() bool {
	return sv.valueType.IsFloat()
}

// IsBool checks if the state value is of boolean type.
//
// Parameters:
//
//	None
//
// Returns:
//
//	(bool) : Indicates whether the StateValue is of boolean type or not.
func (sv StateValue) IsBool() bool {
	return sv.valueType.IsBool()
}

// IsString checks if the underlying value type of a StateValue object
// represents a string.
//
// Parameters:
//   - None.
//
// Returns:
//
//	bool: Indicates whether the underlying value type is a string or not.
//
// Side Effects:
//   - This function does not modify any state. It only reads and returns a boolean value.
//
// Errors:
//   - This function does not return an error, so you don't have to check for errors.
//
// Edge Cases:
//   - If the underlying type of the StateValue is not TypeString,
//     this function will return false as expected.
//
// Usage example:
//
//	state := upnp.StateValue{valueType: upnp.TypeInt}
//	fmt.Println(state.IsString())  // Outputs: false
func (sv StateValue) IsString() bool {
	return sv.valueType.IsString()
}

// IsTime reports whether this state value represents a time instance.
//
// The function returns true if and only if the underlying type of the
// StateValue is TypeTime, else false. This method does not check for other
// types that are convertible to Time as it's assumed that these would be
// handled by the ConvertToType method beforehand.
//
// No side effects: this function is pure and doesn't change any state.
//
// Errors: This function does not return an error, so you don't have to check
// for errors. However, note that TypeTime conversion might fail if the
// StateValue isn't convertible to Time; use the ConvertToType method in such
// cases.
//
// Edge cases: If the underlying type of the StateValue is not TypeTime or is a
// non-convertible time, this function will return false as expected.
//
// Usage example:
//
//	state := upnp.StateValue{valueType: upnp.TypeInt}
//	fmt.Println(state.IsTime())
//
// Outputs: false
func (sv StateValue) IsTime() bool {
	return sv.valueType.IsTime()
}

// IsUUID checks if the state value type is a UUID (Universally Unique
// Identifier).
//
// It returns true if the underlying type of the StateValue object represents a
// UUID; false otherwise.
//
// Returns: bool: Indicates whether the StateValue is of UUID type or not.
func (sv StateValue) IsUUID() bool {
	return sv.valueType.IsUUID()
}

// IsURI checks if the state value type is a URI.
// The method returns a boolean indicating whether the state value type represents
// a Uniform Resource Identifier (URI) or not.
//
// Returns: bool: Indicates whether the StateValue is of URI type or not.
func (sv StateValue) IsURI() bool {
	return sv.valueType.IsURI()
}

// IsBinary checks if the state value type is binary or not.
//
// The function returns a boolean indicating whether the provided StateValue's
// value type is of binary format (bin.base64 or bin.hex).
//
// Returns: bool: If the underlying value type of the StateValue object is
// either TypeBinBase64 or TypeBinHex, this method will return true; otherwise,
// it returns false.
func (sv StateValue) IsBinary() bool {
	return sv.valueType.IsBinary()
}

// IsComparable function checks if a state variable is comparable or not.
//
// It returns false for binary types (bin.base64 and bin.hex)
// as they are non-comparable. For all other types, it returns true indicating
// that these types can be compared.
// //
// Returns: bool: A boolean value indicating whether the given StateValue is
// comparable or not. True means it's comparable, False means it isn't.
func (sv StateValue) IsComparable() bool {
	return sv.valueType.IsComparable()
}
