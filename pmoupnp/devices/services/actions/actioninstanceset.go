package actions

import (
	"iter"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/objectstore"
	"github.com/beevik/etree"
)

type ActionInstanceSet objectstore.ObjectSet[*ActionInstance]

func (m *ActionInstanceSet) Insert(obj *ActionInstance) {
	(*objectstore.ObjectSet[*ActionInstance])(m).Insert(obj)
}

func (set *ActionInstanceSet) Contains(obj *ActionInstance) bool {
	return (*objectstore.ObjectSet[*ActionInstance])(set).Contains(obj)
}

func (m *ActionInstanceSet) All() iter.Seq[*ActionInstance] {
	return (*objectstore.ObjectSet[*ActionInstance])(m).All()
}

func (m *ActionInstanceSet) ToXMLElement() *etree.Element {
	elem := etree.NewElement("actionList")

	for sv := range m.All() {
		elem.AddChild(sv.ToXMLElement())
	}

	return elem
}
