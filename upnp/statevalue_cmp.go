package upnp

func (sv StateValue) Cmp(a, b interface{}) (int, error) {
	return sv.valueType.Cmp(a, b)
}

func (sv StateValue) Equal(a, b interface{}) (int, error) {
	return sv.valueType.Cmp(a, b)
}

func (sv StateValue) InRange(val interface{}, interval *ValueRange) (bool, error) {
	return sv.valueType.InRange(val, interval)
}
