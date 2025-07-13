package statevariables

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

// Sub subtracts 'b' from 'a'. It converts both values to numeric types
// and then performs a subtraction operation, casting the result back to its
// original type. If either conversion fails or an unsupported type is used,
// it returns an error.
func (t StateVarType) Sub(a, b interface{}) (interface{}, error) {
	af, bf, err := valuesToNumericOperands(t, a, b)
	if err != nil {
		return nil, err
	}

	return t.Cast(af - bf)
}

// Mul takes in two interface types 'a' and 'b', multiplies them together
// and returns the result along with any error encountered during this
// process. If either of the inputs is not compatible with numeric values, an
// error will be returned. The multiplication operation is performed between two
// numbers represented as 'float64' types (since Go does not support generic
// types on its own). The resulting value will be cast to the type represented
// by the receiver of this method 't'. If a casting error occurs, it will also
// be returned along with nil for the result.
func (t StateVarType) Mul(a, b interface{}) (interface{}, error) {
	af, bf, err := valuesToNumericOperands(t, a, b)
	if err != nil {
		return nil, err
	}

	return t.Cast(af * bf)
}

// Div performs division between the provided arguments 'a' and 'b'. The function casts both operands to their numeric equivalents using valuesToNumericOperands() before performing the division.
//
// Parameters:
//   - a: first operand of type interface{}, can be of any type, will be converted if necessary
//   - b: second operand of type interface{}, can be of any type, will be converted if necessary
//
// Returns:
//   - result: the division result in numeric form after casting it with function t.Cast()
//   - err: error that might occur during the conversion or division operation
func (t StateVarType) Div(a, b interface{}) (interface{}, error) {
	af, bf, err := valuesToNumericOperands(t, a, b)
	if err != nil {
		return nil, err
	}

	return t.Cast(af / bf)
}
