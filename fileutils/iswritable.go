package fileutils

import (
	"os"
	"path/filepath"
)

func IsWriteable(path string) bool {
	info, err := os.Stat(path)
	if err == nil {
		// File exists, check owner write permission
		return info.Mode().Perm()&0200 != 0
	}
	if os.IsNotExist(err) {
		// File does not exist, check if parent directory is writable
		dir := filepath.Dir(path)
		if dir == "" {
			dir = "." // fallback
		}
		dirInfo, err := os.Stat(dir)
		if err != nil {
			return false
		}
		return dirInfo.IsDir() && dirInfo.Mode().Perm()&0200 != 0
	}
	return false
}
