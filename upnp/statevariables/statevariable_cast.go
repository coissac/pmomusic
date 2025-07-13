package statevariables

// Cast transforms any given value into an interface suitable for use in a StateValue object, using
// the underlying type's specific casting rules. It will return an error if the transformation fails or
// if the provided interface is not supported by the ValueType of the StateValue.
// This function does NOT mutate the original value it receives as input.
//
// The parameter 'val' is an interface that needs to be cast into a form compatible with the internal
// state representation used by the StateValue object.
//
// It returns an interface and an error: if the casting operation was successful, the first return value will
// be the casted version of 'val', and the second one (an error) will be nil. If the casting fails, it will
// return a nil for the first value and an appropriate error for the second one.
//
// This function should not modify its input value. It is idempotent and always returns consistent results given
// the same inputs. However, if the provided interface 'val' does not match with the ValueType of StateValue, it
// will return an error.
//
// Example usage:
//
//	state := upnp.NewStateValue(upnp.NewTime())
//	castedVal, err := state.Cast("2022-12-31")
//	if err != nil {
//	    log.Println(err)
//	    return
//	}
func (sv *StateVariable) Cast(val interface{}) (interface{}, error) {
	return sv.valueType.Cast(val)
}
