package upnp

import (
	"fmt"
	"time"
)

func toTime(v interface{}) (time.Time, error) {
	switch val := v.(type) {
	case time.Time:
		return val, nil

	case string:
		// Liste de layouts communs à UPnP / ISO-8601 / RFC
		layouts := []string{
			time.RFC3339Nano,       // 2006-01-02T15:04:05.999999999Z07:00
			time.RFC3339,           // 2006-01-02T15:04:05Z07:00
			"2006-01-02",           // date only
			"15:04:05",             // time only
			"15:04:05Z07:00",       // time with TZ
			"2006-01-02T15:04:05",  // no TZ
			"2006-01-02T15:04:05Z", // UTC
			"2006-01-02 15:04:05",  // space-separated
		}

		for _, layout := range layouts {
			if t, err := time.Parse(layout, val); err == nil {
				return t, nil
			}
		}
		return time.Time{}, fmt.Errorf("cannot parse time string: %q", val)

	case int64:
		return time.Unix(val, 0), nil

	case float64:
		sec := int64(val)
		nsec := int64((val - float64(sec)) * 1e9)
		return time.Unix(sec, nsec), nil

	default:
		return time.Time{}, fmt.Errorf("unsupported type for time conversion: %T", v)
	}
}

func cmpTime(a, b time.Time) int {
	switch {
	case a.Before(b):
		return -1
	case a.After(b):
		return 1
	default:
		return 0
	}
}
