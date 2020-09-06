package main

import (
	"bufio"
	"fmt"
	"io"
	"io/ioutil"
	"os"
	"path/filepath"
	"strings"
	"syscall"

	"git.sr.ht/~sircmpwn/getopt"
	"github.com/pelletier/go-toml"
	"golang.org/x/crypto/ssh/terminal"
	"maunium.net/go/mautrix"
	"maunium.net/go/mautrix/event"
	"maunium.net/go/mautrix/id"
)

type runtimeOptions struct {
	room string
	user string

	cmdCreateRoom bool
	cmdInviteUser bool
	cmdLogin      bool
	cmdListRooms  bool
	cmdHelp       bool
}

func parseArgs() runtimeOptions {
	args := runtimeOptions{}
	getopt.BoolVar(&args.cmdCreateRoom, "c", false, "Create a new room")
	getopt.BoolVar(&args.cmdInviteUser, "i", false, "Invite a user to a room")
	getopt.StringVar(&args.room, "r", "", "The RoomID for a few operations")
	getopt.StringVar(&args.user, "u", "", "The UserID for a few operations")
	getopt.BoolVar(&args.cmdLogin, "l", false, "Create a login session on the server")
	getopt.BoolVar(&args.cmdListRooms, "L", false, "List rooms where the user is in")
	getopt.BoolVar(&args.cmdHelp, "h", false, "Show this page and exit")
	getopt.Parse()

	return args
}

func loginPassword(client *mautrix.Client, user id.UserID, password string) (string, error) {
	loginReq := mautrix.ReqLogin{
		Type: mautrix.AuthTypePassword,
		Identifier: mautrix.UserIdentifier{
			Type: mautrix.IdentifierTypeUser,
			User: string(user),
		},
		Password:                 password,
		InitialDeviceDisplayName: "mnotify",
		StoreCredentials:         true,
	}

	resp, err := client.Login(&loginReq)
	if err != nil {
		return "", err
	}
	return resp.AccessToken, nil
}

type config struct {
	UserID      id.UserID
	AccessToken string
}

func configPath() string {
	p, err := os.UserConfigDir()
	if err != nil {
		panic(err)
	}
	return filepath.Join(p, "mnotify", "config.toml")
}

func createClient(user id.UserID, token string) (*mautrix.Client, error) {
	_, homeserver, err := user.Parse()
	if err != nil {
		return nil, err
	}
	wellKnown, err := mautrix.DiscoverClientAPI(homeserver)
	if err != nil {
		return nil, err
	}
	client, err := mautrix.NewClient(wellKnown.Homeserver.BaseURL, user, token)
	if err != nil {
		return nil, err
	}
	return client, nil
}

func loadConfig() (config, error) {
	file, err := os.Open(configPath())
	if err != nil {
		return config{}, err
	}
	confStr, err := ioutil.ReadAll(file)
	if err != nil {
		return config{}, err
	}
	var conf config
	if err := toml.Unmarshal(confStr, &conf); err != nil {
		return config{}, err
	}
	return conf, nil
}

func storeConfig(user id.UserID, accessToken string) error {
	confPath := configPath()
	confDir := filepath.Dir(confPath)
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
`, user, accessToken)
	if _, err := io.Copy(file, strings.NewReader(data)); err != nil {
		return err
	}
	return nil
}

func main() {
	args := parseArgs()
	if args.cmdHelp {
		getopt.Usage()
		os.Exit(0)
	}

	if args.cmdLogin {
		fmt.Print("Username: ")
		u, _ := bufio.NewReader(os.Stdin).ReadString('\n')
		user := id.UserID(strings.TrimSpace(u))

		fmt.Print("User Password: ")
		password, err := terminal.ReadPassword(syscall.Stdin)
		if err != nil {
			fmt.Printf("reading password failed: %s\n", err)
			os.Exit(1)
		}
		client, err := createClient(user, "")
		if err != nil {
			fmt.Printf("client error: %s\n", err)
			os.Exit(1)
		}
		token, err := loginPassword(client, user, string(password))
		if err != nil {
			fmt.Printf("login failed: %s\n", err)
			os.Exit(1)
		}
		if err := storeConfig(user, token); err != nil {
			fmt.Printf("storing config failed: %s\n", err)
			os.Exit(1)
		}
		os.Exit(0)
	}
	conf, err := loadConfig()
	if err != nil {
		fmt.Printf("loading config failed: %s\n", err)
		os.Exit(1)
	}

	client, err := createClient(conf.UserID, conf.AccessToken)
	if err != nil {
		fmt.Printf("client error: %s\n", err)
		os.Exit(1)
	}

	switch {
	case args.cmdCreateRoom:
		req := &mautrix.ReqCreateRoom{
			Preset: "trusted_private_chat",
		}
		resp, err := client.CreateRoom(req)
		if err != nil {
			fmt.Printf("creating room failed: %s\n", err)
			os.Exit(1)
		}
		fmt.Println(resp.RoomID)
	case args.cmdInviteUser:
		req := &mautrix.ReqInviteUser{
			UserID: id.UserID(args.user),
		}
		_, err := client.InviteUser(id.RoomID(args.room), req)
		if err != nil {
			fmt.Printf("inviting user failed: %s\n", err)
			os.Exit(1)
		}
	case args.cmdListRooms:
		rooms, err := client.JoinedRooms()
		if err != nil {
			fmt.Printf("getting room list failed: %s\n", err)
			os.Exit(1)
		}
		for _, rawRoom := range rooms.JoinedRooms {
			room := mautrix.NewRoom(rawRoom)
			event := room.GetStateEvent(event.StateCanonicalAlias, "")
			if event != nil {
				fmt.Println(event.Content.AsCanonicalAlias().Alias)
			} else {
				members, err := client.JoinedMembers(room.ID)
				if err != nil {
					fmt.Printf("error room %s: %s\n", rawRoom, err)
					continue
				}
				fmt.Println(rawRoom)
				for _, v := range members.Joined {
					fmt.Printf("  %s\n", *v.DisplayName)
				}
			}
		}
	default:
		msg, err := ioutil.ReadAll(os.Stdin)
		if err != nil {
			fmt.Printf("reading from stdin failed: %s\n", err)
			os.Exit(1)
		}
		_, err = client.SendText(id.RoomID(args.room), string(msg))
		if err != nil {
			fmt.Printf("sending message failed: %s\n", err)
			os.Exit(1)
		}
	}
}
