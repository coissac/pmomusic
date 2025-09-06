package upnp

import (
	_ "embed"
	"fmt"
	"os"
	"os/user"
	"path"
	"strings"
	"sync"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/fileutils"
	"github.com/google/uuid"
	log "github.com/sirupsen/logrus"
	"gopkg.in/yaml.v3"
)

//go:embed pmomusic.yaml
var defaultConfig []byte

type Config struct {
	path   string
	mutex  sync.Mutex
	config map[string]interface{}
}

var _CONFIG *Config

const envConfigFile = "PMOMUSIC_CONFIG"
const envPrefix = "PMOMUSIC_CONFIG__"

// LoadConfig loads a configuration file from the given path or a default
// location.
//
// It prioritizes paths in this order:
//   - the provided path,
//   - the file specified by the environment variable PMOMUSIC_CONFIG
//   - the .pmomusic.yml file in the current directory
//   - the .pmomusic.yml file in the user's home directory, and . If no path is found
//
// or it fails to read any of these files, it falls back on a default
// configuration.
//
// Parameters:
//   - path string: The path to the configuration file. If this
//     parameter is empty, the function will look for the configuration in the
//     current user's home directory and an environment variable.
//
// Returns: 1) Config: A struct containing the loaded configuration data.
//
// Side Effects:
//
//   - This function reads from disk, logs informational messages, and may also
//     panic if there are issues unmarshalling the YAML config file.
//
// Errors:
//
//   - The function will log a warning message and continue with a default
//     configuration if it fails to read or unmarshal any of the files. It does not
//     return an error in this case, as returning errors from within deferred
//     functions can cause unexpected behavior.
//
//   - If there's an issue reading or unmarshalling the YAML file, the function
//     will panic as this is a fatal condition that should be addressed immediately.
//
// Edge Cases:
//
//   - This function does not handle race conditions where the
//     configuration file could change between when it checks if the path is empty
//     and when it attempts to read from it. If the file changes in that time, an
//     error will occur.
//
//   - This function assumes the YAML config files are formatted correctly. If
//     they're not, unmarshalling them into a struct may result in unexpected
//     behavior or errors.
//
// Usage: ``` go cfg := LoadConfig("/path/to/config") fmt.Println(cfg) ````
func LoadConfig(filename string) *Config {
	var data []byte
	var err error
	var cfg = &Config{}

	cfg.mutex.Lock()
	defer cfg.mutex.Unlock()

	path := filename

	if path != "" {
		log.Infof("✅ Trying to load config %s", path)
		data, err = os.ReadFile(path)
		if err != nil {
			log.Warnf("❌ cannot read config file %s", path)
			path = ""
		}
	}

	if path == "" {
		path = os.Getenv(envConfigFile)
		if path != "" {
			log.Infof("✅ Trying to load config specified in env var %s", envConfigFile)
			data, err = os.ReadFile(path)
			if err != nil {
				log.Warnf("❌ cannot read config file %s specified in env var %s", path, envConfigFile)
				path = ""
			}
		}
	}

	if path == "" {
		path = ".pmomusic.yml"
		dir, err := os.Getwd()
		if err != nil {
			dir = "."
		}
		log.Infof("✅ Trying to load config file %s/.pmomusic.yml", dir)
		data, err = os.ReadFile(path)
		if err != nil {
			log.Warnf("❌ I cannot read config file %s/.pmomusic.yml", dir)
			path = ""
		}
	}

	if path == "" {
		path = getHomeYmlPath()
		log.Infof("✅ Trying to load config file from user's home %s", path)
		data, err = os.ReadFile(path)
		if err != nil {
			log.Warnf("❌ I cannot read config file %s", path)
			path = ""
		}
	}

	if path == "" {
		log.Infof("✅ Using default embeded config")
		data = defaultConfig
	}

	if err := yaml.Unmarshal(data, &cfg.config); err != nil {
		log.Panicf("invalid YAML config: %w", err)
	}

	cfg.config = lowerKeysMap(cfg.config)

	applyEnvOverrides(cfg)

	if path == "" {
		switch {
		case filename != "" && fileutils.IsWriteable(filename):
			path = filename
		case os.Getenv(envConfigFile) != "" && fileutils.IsWriteable(os.Getenv(envConfigFile)):
			path = os.Getenv(envConfigFile)
		case fileutils.IsWriteable(".pmomusic.yml"):
			path = ".pmomusic.yml"
		case fileutils.IsWriteable(getHomeYmlPath()):
			path = getHomeYmlPath()
		}
	} else {
		if !fileutils.IsWriteable(path) {
			path = ""
		}
	}

	if path == "" {
		log.Panic("I cannot find a place to store config file")
	}

	log.Infof("✅ Config file will be stored in  %s", path)

	cfg.path = path
	cfg.mutex.Unlock()
	cfg.Save()
	cfg.mutex.Lock()
	return cfg

}

func (cfg *Config) Save() error {
	cfg.mutex.Lock()
	defer cfg.mutex.Unlock()

	cfg.config = lowerKeysMap(cfg.config)

	data, err := yaml.Marshal(cfg.config)
	if err != nil {
		return err
	}

	return os.WriteFile(cfg.path, data, 0644)
}

func (cfg *Config) SetValue(path []string, value interface{}) {
	cfg.setValue(path, value)
	cfg.Save()
}

func (cfg *Config) GetValue(path []string) (interface{}, error) {
	cfg.mutex.Lock()
	defer cfg.mutex.Unlock()

	current := cfg.config
	for i, key := range path {
		key = strings.ToLower(key)

		next, ok := current[key]
		if !ok {
			return nil, fmt.Errorf("path %s does not exist", strings.Join(path[:i+1], "."))
		}
		if i < len(path)-1 {
			current, ok = next.(map[string]interface{})
			if !ok {
				return nil, fmt.Errorf("path  %s is not a Config", strings.Join(path[:i+1], "."))
			}
			continue
		}
		return next, nil
	}
	return nil, fmt.Errorf("path %s does not exist", strings.Join(path[:], "."))
}

// overrideConfig sets a value in a nested map[string]interface{} at the given path.
func (cfg *Config) setValue(path []string, value interface{}) {
	cfg.mutex.Lock()
	defer cfg.mutex.Unlock()

	current := cfg.config
	for i, key := range path {
		key = strings.ToLower(key)
		if i == len(path)-1 {
			current[key] = value
			return
		}
		// ensure intermediate maps exist
		if _, ok := current[key]; !ok {
			current[key] = make(map[string]interface{})
		}
		next, ok := current[key].(map[string]interface{})
		if !ok {
			// If the path conflicts with a non-object, overwrite it
			next = make(map[string]interface{})
			current[key] = next
		}
		current = next
	}
}

// getHomeYmlPath constructs and returns the path to the home directory of the
// current user followed by ".pmomusic.yml".
//
// This function does not take any parameters but it relies on the
// `user.Current()` function, which can return an error if it fails to determine
// the current user or their home directory. In such cases, a description of the
// error is printed to standard output and the empty string is returned.
//
// The function returns a string representing the path to the file in the
// following format: "$HOME/.pmomusic.yml". It does not return an error value
// since no errors are expected to occur during normal operation.
//
// Side Effects: None. This is a pure function that only depends on input and
// produces output without changing any state or causing side effects, except
// for the printing of potential error messages.
//
// Edge Cases: If `user.Current()` fails to determine the current user's home
// directory or the current user, an empty string is returned and a description
// of the error is printed.
//
// Example usage:
//
//	fmt.Println(getHomeYmlPath())
//
// This will print something like "/Users/username/.pmomusic.yml" on macOS or Linux, or "C:\Users\Username\" on Windows if the current user's home directory is "C:\Users\Username".
func getHomeYmlPath() string {
	usr, err := user.Current()
	if err != nil {
		fmt.Println(err)
	}
	return path.Join(usr.HomeDir, ".pmomusic.yml")
}

func applyEnvOverrides(cfg *Config) {
	for _, env := range os.Environ() {
		if !strings.HasPrefix(env, envPrefix) {
			continue
		}

		// Split env var into key and value
		parts := strings.SplitN(env, "=", 2)
		if len(parts) != 2 {
			continue
		}

		keyPath := strings.Split(strings.TrimPrefix(parts[0], envPrefix), "__")
		value := parts[1]

		overrideConfig(cfg, keyPath, value)
	}
}

func convertYAMLScalar(s string) interface{} {
	var out interface{}
	err := yaml.Unmarshal([]byte(s), &out)
	if err != nil {
		// fallback: keep string if parsing failed
		return s
	}
	return out
}

func overrideConfig(cfg *Config, keyPath []string, value string) {
	iv := convertYAMLScalar(value)
	cfg.setValue(keyPath, iv)
}

func lowerKeysMap(m map[string]interface{}) map[string]interface{} {
	out := make(map[string]interface{})
	for k, v := range m {
		lk := strings.ToLower(k)
		// si c'est une map imbriquée, traiter récursivement
		switch vv := v.(type) {
		case map[string]interface{}:
			out[lk] = lowerKeysMap(vv)
		default:
			out[lk] = v
		}
	}
	return out
}

func GetConfig() *Config {

	if _CONFIG == nil {
		_CONFIG = LoadConfig("")
	}

	return _CONFIG
}

func (conf *Config) GetBaseURL() string {
	url, _ := conf.GetValue([]string{"host", "base_url"})
	surl, ok := url.(string)
	if !ok {
		return ""
	}
	return surl
}

func (conf *Config) GetHTTPPort() int {
	port, _ := conf.GetValue([]string{"host", "http_port"})

	iport, ok := port.(int)
	if !ok {
		return 1900
	}

	return iport
}

func (conf *Config) GetDeviceUDN(devtype DeviceType, name string) string {
	udn, error := conf.GetValue([]string{"devices", string(devtype), name, "udn"})

	if error != nil {
		udn = uuid.New().String()
		conf.SetValue([]string{"devices", string(devtype), name, "udn"}, udn)
		conf.Save()
	}

	return udn.(string)
}
