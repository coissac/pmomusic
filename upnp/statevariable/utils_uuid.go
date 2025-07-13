package stateVariables

import (
	"fmt"

	"github.com/google/uuid"
)

// toUUID converts a value to uuid.UUID if possible.
func toUUID(v interface{}) (uuid.UUID, error) {
	switch val := v.(type) {
	case uuid.UUID:
		return val, nil

	case string:
		u, err := uuid.Parse(val)
		if err != nil {
			return uuid.UUID{}, fmt.Errorf("invalid UUID string %q: %v", val, err)
		}
		return u, nil

	case []byte:
		u, err := uuid.FromBytes(val)
		if err != nil {
			return uuid.UUID{}, fmt.Errorf("invalid UUID bytes: %v", err)
		}
		return u, nil

	default:
		return uuid.UUID{}, fmt.Errorf("cannot convert type %T to UUID", v)
	}
}
