package actions

import (
	"iter"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/objectstore"
	"github.com/beevik/etree"
)

type ArgumentSet objectstore.ObjectSet[*Argument]

func (m *ArgumentSet) Insert(obj *Argument) {
	(*objectstore.ObjectSet[*Argument])(m).Insert(obj)
}

func (set *ArgumentSet) Contains(obj *Argument) bool {
	return (*objectstore.ObjectSet[*Argument])(set).Contains(obj)
}

func (m *ArgumentSet) All() iter.Seq[*Argument] {
	return (*objectstore.ObjectSet[*Argument])(m).All()
}

func (m *ArgumentSet) ToXMLElement() *etree.Element {
	elem := etree.NewElement("argumentList")

	for sv := range m.All() {
		elem.AddChild(sv.ToXMLElement())
	}

	return elem
}
