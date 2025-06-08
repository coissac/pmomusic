package ssdp

import (
	"os"
	"path/filepath"

	"github.com/google/uuid"
)

var __UUID = ""

func loadOrCreateUUID() (string, error) {
	configDir, err := os.UserConfigDir()
	if err != nil {
		return "", err
	}
	filePath := filepath.Join(configDir, "pmomusic", "uuid.txt")
	os.MkdirAll(filepath.Dir(filePath), 0755)

	if data, err := os.ReadFile(filePath); err == nil && len(data) > 0 {
		return string(data), nil
	}

	id := "uuid:pmo-" + uuid.New().String()
	if err := os.WriteFile(filePath, []byte(id), 0600); err != nil {
		return "", err
	}
	return id, nil
}

func GetUUID() string {
	if __UUID == "" {
		u, err := loadOrCreateUUID()

		if err != nil {
			panic(err)
		}

		__UUID = u
	}
	return __UUID
}
