package actions

import (
	"iter"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/objectstore"
	"github.com/beevik/etree"
)

type ActionSet objectstore.ObjectSet[*Action]

func (m *ActionSet) Insert(obj *Action) {
	(*objectstore.ObjectSet[*Action])(m).Insert(obj)
}

func (set *ActionSet) Contains(obj *Action) bool {
	return (*objectstore.ObjectSet[*Action])(set).Contains(obj)
}

func (m *ActionSet) All() iter.Seq[*Action] {
	return (*objectstore.ObjectSet[*Action])(m).All()
}

func (m *ActionSet) ToXMLElement() *etree.Element {
	elem := etree.NewElement("ActionList")

	for sv := range m.All() {
		elem.AddChild(sv.ToXMLElement())
	}

	return elem
}
