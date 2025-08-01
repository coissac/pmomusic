package objectstore

import (
	"iter"

	"github.com/google/uuid"
	log "github.com/sirupsen/logrus"
)

type ObjectIdSet map[uuid.UUID]struct{}

func (set ObjectIdSet) Contains(id uuid.UUID) bool {
	_, ok := set[id]
	return ok
}

func (set ObjectIdSet) Add(id uuid.UUID) {
	set[id] = struct{}{}
}

func (set ObjectIdSet) Remove(id uuid.UUID) {
	delete(set, id)
}

func (set ObjectIdSet) Clear() {
	for k := range set {
		delete(set, k)
	}
}

func (set ObjectIdSet) Len() int {
	return len(set)
}

func (set ObjectIdSet) All() iter.Seq[uuid.UUID] {
	return func(yield func(uuid.UUID) bool) {
		for uuid := range set {
			if !yield(uuid) {
				return
			}
		}
	}
}

var objectstore = make(map[uuid.UUID]Object)

func RegisterObject(o Object) uuid.UUID {
	id := uuid.New()
	objectstore[id] = o

	return id
}

func GetObject(id uuid.UUID) (Object, bool) {
	o, ok := objectstore[id]
	return o, ok
}

func RemoveObject(id uuid.UUID) {
	if _, ok := objectstore[id]; !ok {
		delete(objectstore, id)
		return
	}
	log.Warnf("Object %s not found in objectstore", id)
}

func CountOfStoredObjects() int {
	return len(objectstore)
}
