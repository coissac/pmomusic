package statevariables

func (sv StateVariable) Cmp(a, b interface{}) (int, error) {
	return sv.valueType.Cmp(a, b)
}

func (sv StateVariable) Equal(a, b interface{}) (int, error) {
	return sv.valueType.Cmp(a, b)
}

func (sv StateVariable) InRange(val interface{}, interval *ValueRange) (bool, error) {
	return sv.valueType.InRange(val, interval)
}
