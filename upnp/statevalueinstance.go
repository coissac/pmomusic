package upnp

import (
	"sync"
	"time"

	"github.com/beevik/etree"
)

type StateValueInstance struct {
	model         *StateValue
	value         interface{}
	previousValue interface{}
	lastChange    time.Time
	lastEvent     time.Time
	mu            sync.RWMutex
}

func (instance *StateValueInstance) Cast(val interface{}) (interface{}, error) {
	return instance.model.Cast(val)
}

func (instance *StateValueInstance) Model() *StateValue {
	return instance.model
}

func (instance *StateValueInstance) Value() interface{} {
	instance.mu.RLock()
	defer instance.mu.RUnlock()
	return instance.value
}

func (instance *StateValueInstance) SetValue(val interface{}) error {
	cval, err := instance.Cast(val)

	if err != nil {
		return err
	}

	instance.mu.Lock()
	defer instance.mu.Unlock()
	instance.previousValue = instance.value
	instance.value = cval
	return nil
}

func (instance *StateValueInstance) Incr() {
	instance.mu.Lock()
	defer instance.mu.Unlock()

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

func (sv *StateValueInstance) GenerateEvent() *etree.Element {

	// Construire le XML d'événement
	propSet := etree.NewElement("e:propertyset")
	propSet.CreateAttr("xmlns:e", "urn:schemas-upnp-org:event-1-0")

	prop := propSet.CreateElement("e:property")
	elem := prop.CreateElement(sv.model.Name())
	elem.SetText(sv.model.valueToString(sv.Value()))

	return propSet

}
