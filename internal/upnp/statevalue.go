package upnp

import (
	"fmt"
	"reflect"
	"slices"
	"strings"
	"time"

	log "github.com/sirupsen/logrus"
)

type EventType string

const (
	EventEnabled     EventType = "yes"
	EventDisabled    EventType = "no"
	EventConditional EventType = "conditional"
)

type StateConditionFunc func(instance *StateValueInstance) bool

type TypeModifier int

const (
	ModifierAtomic TypeModifier = iota // Valeur simple (par défaut)
	ModifierList                       // Liste d'éléments
	ModifierMap                        // Tableau associatif
	ModifierStruct                     // Structure nommée
)

// StateValue represents a UPnP state variable with value constraints and eventing capabilities.
// It encapsulates:
//   - Value type (StateVarType)
//   - Value range constraints (min/max)
//   - Allowed value list
//   - Eventing configuration
//     graph TD
//     A[StateValue] --> B[baseType: Type sérialisé]
//     A --> C[modifier: Structure des données]
//     C --> D[Liste]
//     C --> E[Map]
//     C --> F[Structure]
//     D --> G[elementType: Type des éléments]
//     E --> H[keyType: Type des clés]
//     E --> I[elementType: Type des valeurs]
//
// Usage:
//
//	state := StateValue{
//	    name: "Volume",
//	    baseType: StateType_UI2,
//	}
//	state.SetRange(0, 100)   // Set 0-100 range
//	state.AppendAllowedValue(25, 50, 75)  // Add specific allowed values
type StateValue struct {
	name            string
	baseType        StateVarType // Renommé pour plus de clarté
	modifier        TypeModifier
	step            interface{}
	minDelta        interface{}
	modifiable      bool
	eventConditions []StateConditionFunc
	description     string
	defaultValue    interface{}
	valueRange      *ValueRange
	allowedValues   []interface{}
	sendEvents      bool
	elementType     StateVarType            // Pour listes et valeurs de maps
	keyType         StateVarType            // Type des clés (pour les maps)
	structFields    map[string]StateVarType // Champs (pour les structures)

}

// Name returns the state variable's name (e.g., "Volume", "Brightness").
func (state *StateValue) Name() string {
	return state.name
}

// Type returns the UPnP data type of the state variable.
func (state *StateValue) Type() StateVarType {
	return state.baseType
}

func (state *StateValue) AddEventCondition(condition StateConditionFunc) {
	state.eventConditions = append(state.eventConditions, condition)
}

// ClearEventConditions réinitialise toutes les conditions
func (state *StateValue) ClearEventConditions() {
	state.eventConditions = nil
}

func (state *StateValue) SetDefault(value interface{}) error {
	if state.IsValidValue(value) {
		cvalue, _ := state.baseType.Cast(value)
		state.defaultValue = cvalue
		log.Debugf("🐞 Setting default value for %v to %v", state.name, cvalue)
		return nil
	}
	return fmt.Errorf("invalid default value for %v (%v)", state.name, value)
}

func (state *StateValue) HasDefault() bool {
	return state.defaultValue != nil
}

func (state *StateValue) DefaultValue() interface{} {
	if !state.HasDefault() {
		return state.baseType.DefaultValue()
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
	limits, err := state.baseType.ValueRange(min, max)
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
	cvalue, err := state.baseType.Cast(value)
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
	cvalue, err := state.baseType.Cast(value)
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
func (state *StateValue) IsValueInRange(value interface{}) bool {
	return state.baseType.InRange(value, state.valueRange)
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
		cv, err := state.baseType.Cast(v)
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
func (state *StateValue) IsValueAllowed(value interface{}) bool {
	if !state.HasAllowedValues() {
		return true // No list = any value valid
	}

	cvalue, err := state.baseType.Cast(value)
	if err != nil {
		return false
	}

	for _, allowed := range state.allowedValues {
		if reflect.DeepEqual(cvalue, allowed) {
			return true
		}
	}
	return false
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
func (state *StateValue) IsValidValue(value interface{}) bool {
	cvalue, err := state.baseType.Cast(value)
	if err != nil {
		return false
	}
	return state.IsValueInRange(cvalue) && state.IsValueAllowed(cvalue)
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
	if _, err := state.baseType.Cast(step); err != nil {
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
