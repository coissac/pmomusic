package pmocover

import "gargoton.petite-maison-orange.fr/eric/pmomusic/pmoconfig"

// GetCoverCache récupère le cache, crée le dossier et la base si nécessaire
func GetCoverCache() (*Cache, error) {
	config := pmoconfig.GetConfig()

	dir := config.GetCoverCacheDir()
	size := config.GetCoverCacheSize()
	cache, err := NewCache(dir, size)
	if err != nil {
		return nil, err
	}
	return cache, nil
}
