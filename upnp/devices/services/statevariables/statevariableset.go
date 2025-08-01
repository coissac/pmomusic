package statevariables

import (
	"iter"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/objectstore"
	"github.com/beevik/etree"
)

type StateVariableSet objectstore.ObjectSet[*StateVariable]

func (m *StateVariableSet) Insert(obj *StateVariable) {
	(*objectstore.ObjectSet[*StateVariable])(m).Insert(obj)
}

func (set *StateVariableSet) Contains(obj *StateVariable) bool {
	return (*objectstore.ObjectSet[*StateVariable])(set).Contains(obj)
}

func (m *StateVariableSet) All() iter.Seq[*StateVariable] {
	return (*objectstore.ObjectSet[*StateVariable])(m).All()
}

func (m *StateVariableSet) ToXMLElement() *etree.Element {
	elem := etree.NewElement("serviceStateTable")

	for sv := range m.All() {
		elem.AddChild(sv.ToXMLElement())
	}

	return elem
}
