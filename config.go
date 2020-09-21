package main

import (
	"fmt"
	"io"
	"io/ioutil"
	"os"
	"path/filepath"
	"strings"

	"github.com/pelletier/go-toml"
	"maunium.net/go/mautrix/id"
)

type config struct {
	UserID      id.UserID
	AccessToken string
	DefaultRoom id.RoomID
}

func configPath() string {
	p, err := os.UserConfigDir()
	if err != nil {
		panic(err)
	}
	return filepath.Join(p, "mnotify", "config.toml")
}

var globalConfigPath = "/etc/mnotify/config.toml"

func loadConfig() (*config, error) {
	var (
		err  error
		file *os.File
	)
	file, err = os.Open(configPath())
	if err != nil {
		file, err = os.Open(globalConfigPath)
		if err != nil {
			return nil, err
		}
	}
	confStr, err := ioutil.ReadAll(file)
	if err != nil {
		return nil, err
	}
	var conf config
	if err := toml.Unmarshal(confStr, &conf); err != nil {
		return nil, err
	}
	return &conf, nil
}

func storeConfig(user id.UserID, deviceID id.DeviceID, accessToken string) error {
	var (
		confPath = configPath()
		confDir  = filepath.Dir(confPath)
	)
	if _, err := os.Stat(confDir); os.IsNotExist(err) {
		if err := os.MkdirAll(confDir, 0700); err != nil {
			return err
		}
	}
	file, err := os.Create(confPath)
	if err != nil {
		return err
	}
	data := fmt.Sprintf(`UserID = "%s"
AccessToken = "%s"
DeviceID = "%s"
`, user, accessToken, deviceID)
	if _, err := io.Copy(file, strings.NewReader(data)); err != nil {
		return err
	}
	return nil
}
