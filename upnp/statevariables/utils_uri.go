package statevariables

import (
	"fmt"
	"net/url"
)

// toURI converts a value to *url.URL if possible.
func toURI(v interface{}) (*url.URL, error) {
	switch val := v.(type) {
	case *url.URL:
		// Already a URL
		return val, nil

	case string:
		u, err := url.Parse(val)
		if err != nil {
			return nil, fmt.Errorf("invalid URI string %q: %v", val, err)
		}
		return u, nil

	default:
		return nil, fmt.Errorf("cannot convert type %T to URI", v)
	}
}
