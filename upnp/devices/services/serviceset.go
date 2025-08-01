package services

import (
	"iter"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/objectstore"
	"github.com/beevik/etree"
)

type ServiceSet objectstore.ObjectSet[*Service]

func (m *ServiceSet) Insert(obj *Service) {
	(*objectstore.ObjectSet[*Service])(m).Insert(obj)
}

func (set *ServiceSet) Contains(obj *Service) bool {
	return (*objectstore.ObjectSet[*Service])(set).Contains(obj)
}

func (m *ServiceSet) All() iter.Seq[*Service] {
	return (*objectstore.ObjectSet[*Service])(m).All()
}

func (m *ServiceSet) ToXMLElement() *etree.Element {
	elem := etree.NewElement("ServiceList")

	for sv := range m.All() {
		elem.AddChild(sv.ToXMLElement())
	}

	return elem
}
