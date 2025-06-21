package upnp

import "time"

type StateValueInstance struct {
	model         *StateValue
	value         interface{}
	previousValue interface{}
	lastChange    time.Time
	lastEvent     time.Time
}

func (instance *StateValueInstance) Model() *StateValue {
	return instance.model
}

func (instance *StateValueInstance) Value() interface{} {
	return instance.value
}

// ShouldTriggerEvent vérifie toutes les conditions
func (instance *StateValueInstance) ShouldTriggerEvent() bool {
	for _, condition := range instance.model.eventConditions {
		if !condition(instance) {
			return false
		}
	}
	return true
}
