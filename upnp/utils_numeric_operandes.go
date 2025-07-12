package upnp

import "fmt"

func valuesToNumericOperands(t StateVarType, a interface{}, b interface{}) (float64, float64, error) {
	var err error
	if !t.IsNumeric() {
		return 0, 0, fmt.Errorf("type %v is not numeric", t)
	}

	a, err = t.Cast(a)
	if err != nil {
		return 0, 0, err
	}

	b, err = t.Cast(b)
	if err != nil {
		return 0, 0, err
	}

	af, err := toFloat(a, 64)
	if err != nil {
		return 0, 0, err
	}
	bf, err := toFloat(b, 64)
	if err != nil {
		return 0, 0, err
	}
	return af, bf, nil
}
