package statevariables

import (
	"iter"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/objectstore"
	"github.com/beevik/etree"
)

type StateVarInstanceSet objectstore.ObjectSet[*StateVarInstance]

func (m *StateVarInstanceSet) Insert(obj *StateVarInstance) error {
	return (*objectstore.ObjectSet[*StateVarInstance])(m).Insert(obj)
}

func (m *StateVarInstanceSet) InsertOrReplace(obj *StateVarInstance) {
	(*objectstore.ObjectSet[*StateVarInstance])(m).InsertOrReplace(obj)
}

func (m *StateVarInstanceSet) Contains(obj *StateVarInstance) bool {
	return (*objectstore.ObjectSet[*StateVarInstance])(m).Contains(obj)
}

func (m *StateVarInstanceSet) All() iter.Seq[*StateVarInstance] {
	return (*objectstore.ObjectSet[*StateVarInstance])(m).All()
}

func (m *StateVarInstanceSet) ToXMLElement() *etree.Element {
	elem := etree.NewElement("serviceStateTable")

	for sv := range m.All() {
		elem.AddChild(sv.ToXMLElement())
	}

	return elem
}
