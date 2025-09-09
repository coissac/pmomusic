package pmodidl

import (
	"iter"

	"slices"
	"strings"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmocover"
	log "github.com/sirupsen/logrus"
)

// Méthodes pour DIDLLite

// CacheAllCoverArts met en cache toutes les images de couverture et stocke les clés primaires
func (d *DIDLLite) CacheAllCoverArts(cache *pmocover.Cache) error {
	itemsWithCovers := Filter(d.AllItems(), func(item *Item) bool {
		return item.AlbumArt != ""
	})

	for item := range itemsWithCovers {
		pk, err := cache.EnsureFromURL(item.AlbumArt)
		if err != nil {
			log.Errorf("❌ Erreur lors de la mise en cache de %s: %v", item.AlbumArt, err)
			continue
		}
		log.Infof("✅ Mise en cache de %s : pk[%s]", item.AlbumArt, pk)
		item.AlbumArtPk = pk
	}

	return nil
}

// AllContainers retourne un itérateur sur tous les containers de manière récursive
func (d *DIDLLite) AllContainers() iter.Seq[*Container] {
	return func(yield func(*Container) bool) {
		for _, container := range d.Containers {
			if !yield(&container) {
				return
			}
			for child := range container.AllContainers() {
				if !yield(child) {
					return
				}
			}
		}
	}
}

// AllItems retourne un itérateur sur tous les items de manière récursive
func (d *DIDLLite) AllItems() iter.Seq[*Item] {
	return func(yield func(*Item) bool) {
		for _, item := range d.Items {
			if !yield(&item) {
				return
			}
		}
		for container := range d.AllContainers() {
			for _, item := range container.Items {
				if !yield(&item) {
					return
				}
			}
		}
	}
}

// GetContainerByID retourne un itérateur sur les containers avec l'ID spécifié
func (d *DIDLLite) GetContainerByID(id string) iter.Seq[*Container] {
	return func(yield func(*Container) bool) {
		for container := range d.AllContainers() {
			if container.ID == id {
				if !yield(container) {
					return
				}
			}
		}
	}
}

// GetItemByID retourne un itérateur sur les items avec l'ID spécifié
func (d *DIDLLite) GetItemByID(id string) iter.Seq[*Item] {
	return func(yield func(*Item) bool) {
		for item := range d.AllItems() {
			if item.ID == id {
				if !yield(item) {
					return
				}
			}
		}
	}
}

// FilterContainers filtre les containers selon un prédicat
func (d *DIDLLite) FilterContainers(predicate func(*Container) bool) iter.Seq[*Container] {
	return func(yield func(*Container) bool) {
		for container := range d.AllContainers() {
			if predicate(container) && !yield(container) {
				return
			}
		}
	}
}

// FilterItems filtre les items selon un prédicat
func (d *DIDLLite) FilterItems(predicate func(*Item) bool) iter.Seq[*Item] {
	return func(yield func(*Item) bool) {
		for item := range d.AllItems() {
			if predicate(item) && !yield(item) {
				return
			}
		}
	}
}

// Méthodes pour Container

// AllContainers retourne un itérateur sur tous les containers enfants de manière récursive
func (c *Container) AllContainers() iter.Seq[*Container] {
	return func(yield func(*Container) bool) {
		if !yield(c) {
			return
		}
		for _, child := range c.Containers {
			for container := range child.AllContainers() {
				if !yield(container) {
					return
				}
			}
		}
	}
}

// AllItems retourne un itérateur sur tous les items du container et de ses enfants
func (c *Container) AllItems() iter.Seq[*Item] {
	return func(yield func(*Item) bool) {
		for _, item := range c.Items {
			if !yield(&item) {
				return
			}
		}
		for _, child := range c.Containers {
			for item := range child.AllItems() {
				if !yield(item) {
					return
				}
			}
		}
	}
}

// GetChildContainers retourne un itérateur sur les containers enfants directs
func (c *Container) GetChildContainers() iter.Seq[*Container] {
	return func(yield func(*Container) bool) {
		for _, container := range c.Containers {
			if !yield(&container) {
				return
			}
		}
	}
}

// GetChildItems retourne un itérateur sur les items directs du container
func (c *Container) GetChildItems() iter.Seq[*Item] {
	return func(yield func(*Item) bool) {
		for _, item := range c.Items {
			if !yield(&item) {
				return
			}
		}
	}
}

// Méthodes pour Item

// GetAudioResources retourne un itérateur sur les ressources audio
func (i *Item) GetAudioResources() iter.Seq[Res] {
	return func(yield func(Res) bool) {
		for _, res := range i.Ress {
			if strings.HasPrefix(res.ProtocolInfo, "http-get:*:audio/") {
				if !yield(res) {
					return
				}
			}
		}
	}
}

// GetPrimaryResource retourne la ressource principale (première disponible)
func (i *Item) GetPrimaryResource() iter.Seq[Res] {
	return func(yield func(Res) bool) {
		if len(i.Ress) > 0 {
			if !yield(i.Ress[0]) {
				return
			}
		}
	}
}

// GetMetadata retourne un itérateur sur les métadonnées sous forme de paires clé-valeur
func (i *Item) GetMetadata() iter.Seq2[string, string] {
	return func(yield func(string, string) bool) {
		if i.Title != "" && !yield("title", i.Title) {
			return
		}
		if i.Artist != "" && !yield("artist", i.Artist) {
			return
		}
		if i.Album != "" && !yield("album", i.Album) {
			return
		}
		if i.Genre != "" && !yield("genre", i.Genre) {
			return
		}
		if i.Date != "" && !yield("date", i.Date) {
			return
		}
		if i.OriginalTrackNumber != "" && !yield("trackNumber", i.OriginalTrackNumber) {
			return
		}
		for _, desc := range i.Descs {
			if desc.TrackGain != "" && !yield("replayGain", desc.TrackGain) {
				return
			}
			if desc.TrackPeak != "" && !yield("replayPeak", desc.TrackPeak) {
				return
			}
		}
	}
}

// Fonctions utilitaires

// Filter filtre une séquence selon un prédicat
func Filter[T any](seq iter.Seq[T], predicate func(T) bool) iter.Seq[T] {
	return func(yield func(T) bool) {
		for value := range seq {
			if predicate(value) && !yield(value) {
				return
			}
		}
	}
}

// Map transforme une séquence en appliquant une fonction
func Map[T, U any](seq iter.Seq[T], f func(T) U) iter.Seq[U] {
	return func(yield func(U) bool) {
		for value := range seq {
			if !yield(f(value)) {
				return
			}
		}
	}
}

// Collect collecte tous les éléments d'une séquence dans une slice
func Collect[T any](seq iter.Seq[T]) []T {
	return slices.Collect(seq)
}

// First retourne le premier élément d'une séquence
func First[T any](seq iter.Seq[T]) (T, bool) {
	var zero T
	next, stop := iter.Pull(seq)
	defer stop()

	value, ok := next()
	if !ok {
		return zero, false
	}
	return value, true
}

// Count compte le nombre d'éléments dans une séquence
func Count[T any](seq iter.Seq[T]) int {
	count := 0
	for range seq {
		count++
	}
	return count
}
