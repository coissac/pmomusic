package upnp

import (
	"fmt"
	"iter"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/actions"
	sv "gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/devices/services/statevariables"
)

type Service struct {
	name       string
	identifier string
	version    int

	actions    actions.ActionSet
	stateTable sv.StateVariableSet
}

func NewService(name string) *Service {
	svc := &Service{
		name:       name,
		identifier: name,
		version:    1,
		stateTable: make(sv.StateVariableSet),
		actions:    make(actions.ActionSet),
	}

	return svc
}

func (svc *Service) Name() string {
	return svc.name
}

func (svc *Service) TypeID() string {
	return "Service"
}

func (svc *Service) Identifier() string {
	return svc.identifier
}

func (svc *Service) SetIdentifier(id string) {
	svc.identifier = id
}

func (svc *Service) SetVersion(version int) error {
	if version < 1 {
		return fmt.Errorf("%s", "version must be greater than or equal to 1")
	}
	svc.version = version
	return nil
}

func (svc *Service) Version() int {
	return svc.version
}

func (svc *Service) AddVariable(sv *sv.StateVariable) error {
	return svc.stateTable.Insert(sv)
}

func (svc *Service) ContaintsVariable(sv *sv.StateVariable) bool {
	return svc.stateTable.Contains(sv)
}

func (svc *Service) Variables() iter.Seq[*sv.StateVariable] {
	return svc.stateTable.All()
}

func (svc *Service) AddAction(ac *actions.Action) error {
	return svc.actions.Insert(ac)
}

func (svc *Service) NewInstance() *ServiceInstance {
	instance := &ServiceInstance{
		name:       svc.Name(),
		identifier: svc.Identifier(),
		version:    svc.Version(),

		statevariables: make(sv.StateVarInstanceSet),
		actions:        make(actions.ActionInstanceSet),

		subscribers:   make(map[string]string),
		changedBuffer: make(map[string]interface{}),
		seqid:         make(map[string]uint32),
	}

	for v := range svc.stateTable.All() {
		instance.statevariables.Insert(v.NewInstance())
	}

	for a := range svc.actions.All() {
		instance.actions.Insert(a.NewInstance())
	}

	return instance
}
