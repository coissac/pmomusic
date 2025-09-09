package actions

import (
	"iter"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/upnp/objectstore"
)

type ActionSet objectstore.ObjectSet[*Action]

func (m *ActionSet) Insert(obj *Action) error {
	return (*objectstore.ObjectSet[*Action])(m).Insert(obj)
}

func (m *ActionSet) InsertOrReplace(obj *Action) {
	(*objectstore.ObjectSet[*Action])(m).InsertOrReplace(obj)
}

func (set *ActionSet) Contains(obj *Action) bool {
	return (*objectstore.ObjectSet[*Action])(set).Contains(obj)
}

func (m *ActionSet) All() iter.Seq[*Action] {
	return (*objectstore.ObjectSet[*Action])(m).All()
}
