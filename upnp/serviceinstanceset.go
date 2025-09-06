package upnp

import (
	"iter"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/objectstore"
	"github.com/beevik/etree"
)

type ServiceInstanceSet objectstore.ObjectSet[*ServiceInstance]

func (m *ServiceInstanceSet) Insert(obj *ServiceInstance) error {
	return (*objectstore.ObjectSet[*ServiceInstance])(m).Insert(obj)
}

func (m *ServiceInstanceSet) InsertOrReplace(obj *ServiceInstance) {
	(*objectstore.ObjectSet[*ServiceInstance])(m).InsertOrReplace(obj)
}

func (set *ServiceInstanceSet) Contains(obj *ServiceInstance) bool {
	return (*objectstore.ObjectSet[*ServiceInstance])(set).Contains(obj)
}

func (m *ServiceInstanceSet) All() iter.Seq[*ServiceInstance] {
	return (*objectstore.ObjectSet[*ServiceInstance])(m).All()
}

func (m *ServiceInstanceSet) ToXMLElement() *etree.Element {
	elem := etree.NewElement("ServiceList")

	for sv := range m.All() {
		elem.AddChild(sv.ToXMLElement())
	}

	return elem
}
