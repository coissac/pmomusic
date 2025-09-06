package statevariables

import (
	"iter"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/objectstore"
)

type StateVariableSet objectstore.ObjectSet[*StateVariable]

func (m *StateVariableSet) Insert(obj *StateVariable) error {
	return (*objectstore.ObjectSet[*StateVariable])(m).Insert(obj)
}

func (m *StateVariableSet) InsertOrReplace(obj *StateVariable) {
	(*objectstore.ObjectSet[*StateVariable])(m).InsertOrReplace(obj)
}

func (set *StateVariableSet) Contains(obj *StateVariable) bool {
	return (*objectstore.ObjectSet[*StateVariable])(set).Contains(obj)
}

func (m *StateVariableSet) All() iter.Seq[*StateVariable] {
	return (*objectstore.ObjectSet[*StateVariable])(m).All()
}
