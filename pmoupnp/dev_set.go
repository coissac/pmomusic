package upnp

import (
	"iter"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/objectstore"
	"github.com/beevik/etree"
)

type DeviceInstanceSet objectstore.ObjectSet[*DeviceInstance]

func (m *DeviceInstanceSet) Insert(obj *DeviceInstance) error {
	return (*objectstore.ObjectSet[*DeviceInstance])(m).Insert(obj)
}

func (m *DeviceInstanceSet) InsertOrReplace(obj *DeviceInstance) {
	(*objectstore.ObjectSet[*DeviceInstance])(m).InsertOrReplace(obj)
}

func (set *DeviceInstanceSet) Contains(obj *DeviceInstance) bool {
	return (*objectstore.ObjectSet[*DeviceInstance])(set).Contains(obj)
}

func (m *DeviceInstanceSet) All() iter.Seq[*DeviceInstance] {
	return (*objectstore.ObjectSet[*DeviceInstance])(m).All()
}

func (m *DeviceInstanceSet) ToXMLElement() *etree.Element {
	elem := etree.NewElement("DeviceList")

	for sv := range m.All() {
		elem.AddChild(sv.ToXMLElement())
	}

	return elem
}
