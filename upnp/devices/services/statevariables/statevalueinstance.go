package statevariables

import (
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"net/url"
	"reflect"
	"sync"
	"time"

	"github.com/beevik/etree"
	"github.com/google/uuid"
)

type StateVarInstance struct {
	model           *StateVariable
	name            string
	modifiable      bool
	description     string
	step            interface{} // Step size for incremental state values (e.g., "10")
	defaultValue    interface{}
	valueRange      *ValueRange
	eventConditions map[string]StateConditionFunc
	allowedValues   []interface{}
	sendEvents      bool
	parse           StringValueParser
	marshal         ValueSerializer

	value         interface{}
	previousValue interface{}
	lastChange    time.Time
	lastEvent     time.Time
	mu            sync.RWMutex
}

func (instance *StateVarInstance) Name() string {
	return instance.model.Name()
}

func (sv *StateVarInstance) TypeID() string {
	return "StateVarInstance"
}

func (instance *StateVarInstance) BitSize() int {
	return instance.model.BitSize()
}

func (instance *StateVarInstance) Cast(val interface{}) (interface{}, error) {
	return instance.model.Cast(val)
}

func (instance *StateVarInstance) HasDefault() bool {
	return instance.defaultValue != nil
}

func (instance *StateVarInstance) DefaultValue() interface{} {
	return instance.defaultValue
}

func (instance *StateVarInstance) HasRange() bool {
	return instance.valueRange != nil
}

func (instance *StateVarInstance) Minimum() interface{} {
	if instance.valueRange == nil {
		return nil
	}
	return instance.valueRange.min
}

func (instance *StateVarInstance) Maximum() interface{} {
	if instance.valueRange == nil {
		return nil
	}
	return instance.valueRange.max
}

func (instance *StateVarInstance) IsSendingEvents() bool {
	return instance.sendEvents
}

func (instance *StateVarInstance) HasAllowedValues() bool {
	return len(instance.allowedValues) > 0
}

func (instance *StateVarInstance) AllowedValues() []interface{} {
	return instance.allowedValues
}

// IsValueInRange checks if a value falls within the defined range.
// Always returns true if no range is set.

// Parameters:

// 	value: Value to check

// Returns:

// bool: True if within range or no range defined
func (instance *StateVarInstance) IsValueInRange(value interface{}) (bool, error) {
	return instance.model.valueType.InRange(value, instance.valueRange)
}

func (instance *StateVarInstance) IsValueAllowed(value interface{}) (bool, error) {
	if !instance.HasAllowedValues() {
		return true, nil // No list = any value valid
	}
	cvalue, err := instance.Cast(value)
	if err != nil {
		return false, err
	}

	for _, allowed := range instance.allowedValues {
		if reflect.DeepEqual(cvalue, allowed) {
			return true, nil
		}
	}
	return false, nil
}

func (instance *StateVarInstance) IsValidValue(value interface{}) (bool, error) {
	cvalue, err := instance.Cast(value)
	if err != nil {
		return false, err
	}

	inrange, err1 := instance.IsValueInRange(cvalue)
	allowed, err2 := instance.IsValueAllowed(cvalue)
	if err1 != nil || err2 != nil {
		if err1 != nil {
			err = err1
		} else {
			err = err2
		}
	}
	return inrange && allowed, err
}

func (instance *StateVarInstance) HasDescription() bool {
	return len(instance.description) > 0
}

func (instance *StateVarInstance) Description() string {
	return instance.description
}

func (instance *StateVarInstance) Model() *StateVariable {
	return instance.model
}

func (instance *StateVarInstance) IsConstant() bool {
	return !instance.modifiable
}

func (instance *StateVarInstance) HasStep() bool {
	return instance.step != nil
}

func (instance *StateVarInstance) Step() interface{} {
	return instance.step
}

func (instance *StateVarInstance) Value() interface{} {
	instance.mu.RLock()
	defer instance.mu.RUnlock()
	return instance.value
}

func (instance *StateVarInstance) SetValue(val interface{}) error {
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

func (instance *StateVarInstance) Incr() {
	instance.mu.Lock()
	defer instance.mu.Unlock()

}

// ShouldTriggerEvent vérifie toutes les conditions
func (instance *StateVarInstance) ShouldTriggerEvent() bool {
	for _, condition := range instance.model.eventConditions {
		if !condition(instance) {
			return false
		}
	}
	return true
}

func (sv *StateVarInstance) GenerateEvent() *etree.Element {

	// Construire le XML d'événement
	propSet := etree.NewElement("e:propertyset")
	propSet.CreateAttr("xmlns:e", "urn:schemas-upnp-org:event-1-0")

	prop := propSet.CreateElement("e:property")
	elem := prop.CreateElement(sv.model.Name())
	elem.SetText(sv.valueToString(sv.Value()))

	return propSet

}

// ToXMLElement generates the complete XML representation of the state variable
// Returns an etree.Element that can be serialized to XML
func (sv *StateVarInstance) ToXMLElement() *etree.Element {
	// Create root <stateVariable> element
	elem := etree.NewElement("stateVariable")

	// Add sendEvents attribute (UPnP eventing capability)
	if sv.sendEvents {
		elem.CreateAttr("sendEvents", "yes") // Enable event notifications
	} else {
		elem.CreateAttr("sendEvents", "no") // Disable event notifications
	}

	name := elem.CreateElement("name")
	name.SetText(sv.Name())

	// Add data type element
	dataType := elem.CreateElement("dataType")
	dataType.SetText(sv.model.valueType.String()) // Set UPnP type name (e.g., "ui1", "boolean")

	// Add default value if specified
	if sv.defaultValue != nil {
		defaultValue := elem.CreateElement("defaultValue")
		// Convert value to UPnP-compatible string representation
		defaultValue.SetText(sv.valueToString(sv.defaultValue))
	}

	// Add value range constraints if defined
	if sv.valueRange != nil {
		rangeElem := elem.CreateElement("allowedValueRange")

		// Minimum boundary value
		min := rangeElem.CreateElement("minimum")
		min.SetText(sv.valueToString(sv.valueRange.min))

		// Maximum boundary value
		max := rangeElem.CreateElement("maximum")
		max.SetText(sv.valueToString(sv.valueRange.max))

		// Add step value if defined (for incremental controls)
		if sv.step != nil {
			step := rangeElem.CreateElement("step")
			step.SetText(sv.valueToString(sv.step))
		}
	}

	// Add allowed value list if defined
	if len(sv.allowedValues) > 0 {
		allowedList := elem.CreateElement("allowedValueList")
		for _, value := range sv.allowedValues {
			// Create individual <allowedValue> elements
			allowed := allowedList.CreateElement("allowedValue")
			allowed.SetText(sv.valueToString(value))
		}
	}

	// Add description if available
	if sv.description != "" {
		desc := elem.CreateElement("description")
		desc.SetText(sv.description) // Human-readable description
	}

	return elem
}

// valueToString converts a value to its UPnP-compatible string representation
// Handles type-specific formatting for proper XML serialization
func (sv *StateVarInstance) valueToString(val interface{}) string {
	if val == nil {
		return "" // Safeguard against nil values
	}

	// Type-specific formatting for UPnP compliance
	switch sv.model.valueType {
	case StateType_Boolean:
		// Boolean: "1" for true, "0" for false (UPnP standard)
		if b, ok := val.(bool); ok && b {
			return "1"
		}
		return "0"

	case StateType_Date:
		// Date: YYYY-MM-DD format
		if t, ok := val.(time.Time); ok {
			return t.Format("2006-01-02")
		}

	case StateType_DateTime, StateType_DateTimeTZ:
		// DateTime: ISO 8601 format with timezone
		if t, ok := val.(time.Time); ok {
			return t.Format(time.RFC3339)
		}

	case StateType_Time, StateType_TimeTZ:
		// Time: HH:MM:SS format
		if t, ok := val.(time.Time); ok {
			return t.Format("15:04:05")
		}

	case StateType_BinBase64:
		// Binary: Base64 encoding
		if b, ok := val.([]byte); ok {
			return base64.StdEncoding.EncodeToString(b)
		}

	case StateType_BinHex:
		// Binary: Hex encoding
		if b, ok := val.([]byte); ok {
			return hex.EncodeToString(b)
		}

	case StateType_URI:
		// URI: Full URL string
		if u, ok := val.(*url.URL); ok {
			return u.String()
		}

	case StateType_UUID:
		// UUID: Canonical string representation
		if u, ok := val.(uuid.UUID); ok {
			return u.String()
		}
	}

	// Default conversion for unsupported types or fallback
	return fmt.Sprintf("%v", val)
}
