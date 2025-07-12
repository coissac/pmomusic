package upnp

import (
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"net/url"
	"reflect"
	"slices"
	"strings"
	"time"

	"github.com/beevik/etree"
	"github.com/google/uuid"
	log "github.com/sirupsen/logrus"
)

type EventType string

type StateConditionFunc func(instance *StateValueInstance) bool
type StringValueParser func(value string) (interface{}, error)
type ValueSerializer func(value interface{}) (string, error)

type StateValue struct {
	name            string       // Name of the state value (e.g., "Volume", "Brightness")
	valueType       StateVarType // Type of state value, see the upnp/statevaluetype package for more information on this.
	step            interface{}  // Step size for incremental state values (e.g., "10")
	modifiable      bool
	eventConditions map[string]StateConditionFunc
	description     string
	defaultValue    interface{}
	valueRange      *ValueRange
	allowedValues   []interface{}
	sendEvents      bool
	parse           StringValueParser
	marshal         ValueSerializer
}

// BitSize returns the number of bits that will be used to represent values
// when this type is used in an UPnP State Variable. The returned value can be
// either 8, 16, 24, 32, or 64, depending on whether t is Byte, Boolean, I2, Ui2,
// I4, Ui4 respectively. If none of these types match, it will return -1.
func (sv StateValue) BitSize() int {
	return sv.valueType.BitSize()
}

// Name returns the state variable's name (e.g., "Volume", "Brightness").
func (sv StateValue) Name() string {
	return sv.name
}

// Type returns the UPnP data type of the state variable.
func (state *StateValue) Type() StateVarType {
	return state.valueType
}

func (state *StateValue) AddEventCondition(name string, condition StateConditionFunc) {
	state.eventConditions[name] = condition
}

func (state *StateValue) DeleteEventConditions(name string) error {
	if _, ok := state.eventConditions[name]; !ok {
		return fmt.Errorf("%s: no such event condition (%s)", state.name, name)
	}
	delete(state.eventConditions, name)
	return nil
}

// ClearEventConditions réinitialise toutes les conditions
func (state *StateValue) ClearEventConditions() {
	state.eventConditions = make(map[string]StateConditionFunc)
}

func (sv *StateValue) SetMinDelta(minDelta interface{}) error {
	if minDelta == nil {
		return fmt.Errorf("%s: nil is an invalid minimum delta value", sv.name)
	}

	minDelta, err := sv.Cast(minDelta)
	if err != nil {
		return fmt.Errorf("%s: invalid minimum delta value %v : %v", sv.name, minDelta, err)
	}

	// mdf := func(instance *StateValueInstance) bool {
	// 	o := instance.previousValue
	// 	n := instance.value

	// 	r,err := instance.model.valueType.ValueRange(o, n)

	// 	if err != nil {
	// 		return false
	// 	}

	// 	return instance.model.valueType.InRange()
	// }

	// sv.eventConditions["MinDelta"] = mdf
	return nil
}

func (state *StateValue) SetDefault(value interface{}) error {
	var err error
	var valid bool

	if valid, err = state.IsValidValue(value); valid && err == nil {
		cvalue, _ := state.valueType.Cast(value)
		state.defaultValue = cvalue
		log.Debugf("🐞 Setting default value for %v to %v", state.name, cvalue)
		return nil
	}
	return fmt.Errorf("invalid default value for %v (%v) : %v", state.name, value, err)
}

func (state *StateValue) HasDefault() bool {
	return state.defaultValue != nil
}

func (state *StateValue) DefaultValue() interface{} {
	if !state.HasDefault() {
		return state.valueType.DefaultValue()
	}

	return state.DefaultValue()
}

// HasRange indicates if a value range constraint is defined.
// Returns true if min/max boundaries are set.
func (state *StateValue) HasRange() bool {
	return state.valueRange != nil
}

// Maximum returns the upper bound of the value range.
// Returns nil if no range is defined.
func (state *StateValue) Maximum() interface{} {
	if state.valueRange == nil {
		return nil
	}
	return state.valueRange.max
}

// Minimum returns the lower bound of the value range.
// Returns nil if no range is defined.
func (state *StateValue) Minimum() interface{} {
	if state.valueRange == nil {
		return nil
	}
	return state.valueRange.min
}

// SetRange defines the inclusive value range [min, max].
// Validates and casts values to the state variable's type.
//
// Parameters:
//
//	min: Lower boundary value
//	max: Upper boundary value
//
// Returns:
//
//	error: If values can't be cast to the type or are nil
//
// Example:
//
//	err := volumeState.SetRange(0, 100)  // 0-100 range for volume
func (state *StateValue) SetRange(min, max interface{}) error {
	if min == nil || max == nil {
		return fmt.Errorf("min and max must not be nil")
	}
	limits, err := state.valueType.ValueRange(min, max)
	if err != nil {
		return fmt.Errorf("setting range: %v", err)
	}
	state.valueRange = limits

	log.Debugf("🐞 Setting range of %s to [%v, %v]", state.name, min, max)
	return nil
}

// UpdateMinimalValue dynamically updates the lower range boundary.
// Requires an existing range to be set.
//
// Parameters:
//
//	value: New minimum value
//
// Returns:
//
//	error: If no range exists or value can't be cast
func (state *StateValue) UpdateMinimalValue(value interface{}) error {
	if state.valueRange == nil {
		return fmt.Errorf("no range set for value %v", state.name)
	}
	cvalue, err := state.valueType.Cast(value)
	if err != nil {
		return fmt.Errorf("casting value: %v", err)
	}
	state.valueRange.min = cvalue

	log.Debugf("🐞 Updating minimal value of %s to %v", state.name, cvalue)
	return nil
}

// UpdateMaximalValue dynamically updates the upper range boundary.
// Requires an existing range to be set.
//
// Parameters:
//
//	value: New maximum value
//
// Returns:
//
//	error: If no range exists or value can't be cast
func (state *StateValue) UpdateMaximalValue(value interface{}) error {
	if state.valueRange == nil {
		return fmt.Errorf("no range set for value %v", state.name)
	}
	cvalue, err := state.valueType.Cast(value)
	if err != nil {
		return fmt.Errorf("casting value: %v", err)
	}
	state.valueRange.max = cvalue

	log.Debugf("🐞 Updating maximal value of %s to %v", state.name, cvalue)
	return nil
}

// IsValueInRange checks if a value falls within the defined range.
// Always returns true if no range is set.
//
// Parameters:
//
//	value: Value to check
//
// Returns:
//
//	bool: True if within range or no range defined
func (state *StateValue) IsValueInRange(value interface{}) (bool, error) {
	return state.valueType.InRange(value, state.valueRange)
}

// IsSendingEvents indicates if state changes trigger UPnP events.
func (state *StateValue) IsSendingEvents() bool {
	return state.sendEvents
}

// SetSendingEvents enables event notifications for state changes.
func (state *StateValue) SetSendingEvents() {
	state.sendEvents = true
	log.Debugf("🐞 Enabling event sending for %s", state.name)
}

// UnsetSendingEvents disables event notifications for state changes.
func (state *StateValue) UnsetSendingEvents() {
	state.sendEvents = false
	log.Debugf("🐞 Disabling event sending for %s", state.name)
}

// HasAllowedValues indicates if an allowed value list is defined.
func (state *StateValue) HasAllowedValues() bool {
	return len(state.allowedValues) > 0
}

// AllowedValues returns the list of permitted values.
// Returns an empty slice if no values are defined.
func (state *StateValue) AllowedValues() []interface{} {
	return state.allowedValues
}

// AppendAllowedValue adds values to the permitted value list.
// Values are cast to the state variable's type before adding.
//
// Parameters:
//
//	value: One or more values to add
//
// Returns:
//
//	error: If any value can't be cast to the type
//
// Example:
//
//	err := state.AppendAllowedValue("PLAYING", "PAUSED", "STOPPED")
func (state *StateValue) AppendAllowedValue(value ...interface{}) error {
	state.allowedValues = slices.Grow(state.allowedValues, len(value))
	for _, v := range value {
		cv, err := state.valueType.Cast(v)
		if err != nil {
			return fmt.Errorf("casting allowed value: %v", err)
		}
		state.allowedValues = append(state.allowedValues, cv)
	}

	log.Debugf("🐞 Added allowed values to %s: %v", state.name, value)
	return nil
}

// IsValueAllowed checks if a value exists in the allowed value list.
// Always returns true if no allowed values are defined.
//
// Parameters:
//
//	value: Value to check
//
// Returns:
//
//	bool: True if value is permitted or no list defined
func (state *StateValue) IsValueAllowed(value interface{}) (bool, error) {
	if !state.HasAllowedValues() {
		return true, nil // No list = any value valid
	}

	cvalue, err := state.valueType.Cast(value)
	if err != nil {
		return false, err
	}

	for _, allowed := range state.allowedValues {
		if reflect.DeepEqual(cvalue, allowed) {
			return true, nil
		}
	}
	return false, nil
}

// IsValidValue performs full validation against all constraints.
// Checks (in order):
//  1. Value can be cast to the type
//  2. Value is within range (if defined)
//  3. Value is in allowed list (if defined)
//
// Returns:
//
//	bool: True if value passes all applicable constraints
func (state *StateValue) IsValidValue(value interface{}) (bool, error) {
	cvalue, err := state.valueType.Cast(value)
	if err != nil {
		return false, err
	}

	inrange, err1 := state.IsValueInRange(cvalue)
	allowed, err2 := state.IsValueAllowed(cvalue)
	if err1 != nil || err2 != nil {
		if err1 != nil {
			err = err1
		} else {
			err = err2
		}
	}
	return inrange && allowed, err
}

func (state *StateValue) HasDescription() bool {
	return len(state.description) > 0
}

func (state *StateValue) Description() string {
	return state.description
}

func (state *StateValue) SetDescription(desc string) {
	state.description = strings.TrimSpace(desc)
}

func (state *StateValue) IsConstant() bool {
	return !state.modifiable
}

func (state *StateValue) SetConstant() {
	state.modifiable = false
}

func (state *StateValue) SetModifiable() {
	state.modifiable = true
}

func (state *StateValue) SetStep(step interface{}) error {
	// Validation que le step correspond au type de la variable
	if _, err := state.valueType.Cast(step); err != nil {
		return fmt.Errorf("invalid step type: %v", err)
	}
	state.step = step
	return nil
}

func (state *StateValue) UnsetStep() {
	state.step = nil
}

func (state *StateValue) HasStep() bool {
	return state.step != nil
}

func (state *StateValue) Step() interface{} {
	return state.step
}

func (state *StateValue) NewInstance() *StateValueInstance {
	return &StateValueInstance{
		model:      state,
		value:      state.DefaultValue(),
		lastChange: time.Now(),
		lastEvent:  time.Unix(int64(1718985600), 0).UTC(),
	}
}

// ToXMLElement generates the complete XML representation of the state variable
// Returns an etree.Element that can be serialized to XML
func (sv *StateValue) ToXMLElement() *etree.Element {
	// Create root <stateVariable> element
	elem := etree.NewElement("stateVariable")
	elem.CreateAttr("name", sv.name)

	// Add sendEvents attribute (UPnP eventing capability)
	if sv.sendEvents {
		elem.CreateAttr("sendEvents", "yes") // Enable event notifications
	} else {
		elem.CreateAttr("sendEvents", "no") // Disable event notifications
	}

	// Add data type element
	dataType := elem.CreateElement("dataType")
	dataType.SetText(sv.valueType.String()) // Set UPnP type name (e.g., "ui1", "boolean")

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
func (sv *StateValue) valueToString(val interface{}) string {
	if val == nil {
		return "" // Safeguard against nil values
	}

	// Type-specific formatting for UPnP compliance
	switch sv.valueType {
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
