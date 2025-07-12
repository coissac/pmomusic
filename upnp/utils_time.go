package upnp

import (
	"fmt"
	"strings"
	"time"
)

// toTime converts the given value to a time.Time type if possible, otherwise it returns an error.
// The supported types for conversion are: time.Time, string, int64 and float64.
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

// parseUPnPTime parses time values using UPnP-specific formats:
//   - Date: "2006-01-02"
//   - Time: "15:04:05"
//   - DateTime: "2006-01-02T15:04:05"
//   - TimeTZ: "15:04:05-07:00"
//   - DateTimeTZ: "2006-01-02T15:04:05-07:00"
func parseUPnPTime(t StateVarType, s string) (time.Time, error) {
	s = strings.TrimSpace(s)

	layouts := []string{}

	switch t {
	case StateType_Date:
		layouts = []string{"2006-01-02"}

	case StateType_Time:
		layouts = []string{"15:04:05"} // HH:MM:SS

	case StateType_TimeTZ:
		layouts = []string{"15:04:05Z07:00"} // HH:MM:SS+TZ

	case StateType_DateTime:
		layouts = []string{"2006-01-02T15:04:05"} // ISO8601 sans TZ

	case StateType_DateTimeTZ:
		layouts = []string{
			"2006-01-02T15:04:05Z07:00", // full
			"2006-01-02T15:04:05-0700",  // fallback no colon
			"2006-01-02T15:04:05Z",      // Zulu
		}

	default:
		return time.Time{}, fmt.Errorf("unsupported date/time type: %v", t)
	}

	for _, layout := range layouts {
		if ts, err := time.Parse(layout, s); err == nil {
			return ts, nil
		}
	}
	return time.Time{}, fmt.Errorf("invalid %v value: %q", t, s)
}

// cmpTime compares two time.Times, returning -1 if the first is before the second,
// 1 if the first is after the second, and 0 if they're equal.
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
