package objectstore

import "iter"

type Object interface {
	Name() string
	TypeID() string
}

type ObjectSet[T Object] map[string]T

func (m *ObjectSet[T]) Insert(obj T) {
	(*m)[obj.Name()] = obj
}

func (set *ObjectSet[T]) Contains(obj T) bool {
	_, ok := (*set)[obj.Name()]
	return ok
}

func (m *ObjectSet[T]) All() iter.Seq[T] {
	return func(yield func(T) bool) {
		for _, sv := range *m {
			if !yield(sv) {
				return
			}
		}
	}
}
