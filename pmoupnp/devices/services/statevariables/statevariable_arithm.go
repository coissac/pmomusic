package statevariables

// Add performs addition operation on the given parameters 'a' and 'b'. It calls the
// corresponding method from valueType which is assumed to be an interface that
// provides methods for arithmetic operations. This function returns the result of
// the operation or an error if any occurs during computation. Side effects might
// include modifying the state of the system, but this depends on the actual implementation
// of the valueType's Add method. Errors might occur due to invalid input parameters or
// failures in the addition operation itself. The function does not handle edge cases
// and therefore should be used with caution. Here is an example usage:
//
//	result, err := sv.Add(5, 3)
//	if err != nil {
//		// Handle error
//	} else {
//		// Use result
//	}
func (sv StateVariable) Add(a, b interface{}) (interface{}, error) {
	return sv.valueType.Add(a, b)
}

// Sub performs subtraction operation on the given parameters 'a' and 'b'. It follows
// similar semantics as in the Add method, but for subtraction instead of addition.
func (sv StateVariable) Sub(a, b interface{}) (interface{}, error) {
	return sv.valueType.Sub(a, b)
}

// Mul performs multiplication operation on the given parameters 'a' and 'b'. It follows
// similar semantics as in the Add method, but for multiplication instead of addition.
func (sv StateVariable) Mul(a, b interface{}) (interface{}, error) {
	return sv.valueType.Mul(a, b)
}

// Div performs division operation on the given parameters 'a' and 'b'. It follows similar
// semantics as in the Add method, but for division instead of addition. Please note that
// division by zero is undefined and will result in an error being returned from valueType.Div call.
func (sv StateVariable) Div(a, b interface{}) (interface{}, error) {
	return sv.valueType.Div(a, b)
}
