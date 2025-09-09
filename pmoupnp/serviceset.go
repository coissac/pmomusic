package upnp

import (
	"iter"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/objectstore"
)

type ServiceSet objectstore.ObjectSet[*Service]

func (m *ServiceSet) Insert(obj *Service) error {
	return (*objectstore.ObjectSet[*Service])(m).Insert(obj)
}

func (m *ServiceSet) InsertOrReplace(obj *Service) {
	(*objectstore.ObjectSet[*Service])(m).InsertOrReplace(obj)
}

func (set *ServiceSet) Contains(obj *Service) bool {
	return (*objectstore.ObjectSet[*Service])(set).Contains(obj)
}

func (m *ServiceSet) All() iter.Seq[*Service] {
	return (*objectstore.ObjectSet[*Service])(m).All()
}
