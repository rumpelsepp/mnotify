package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"syscall"

	"github.com/spf13/cobra"
	"golang.org/x/crypto/ssh/terminal"
	"maunium.net/go/mautrix"
	"maunium.net/go/mautrix/id"
)

func loginPassword(client *mautrix.Client, user id.UserID, password string) (*mautrix.RespLogin, error) {
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

	return client.Login(&loginReq)
}

type loginCommand struct {
	globalOpts *globalOptions
	printToken bool
}

func (c *loginCommand) run(cmd *cobra.Command, args []string) error {
	fmt.Print("Username: ")
	u, _ := bufio.NewReader(os.Stdin).ReadString('\n')
	user := id.UserID(strings.TrimSpace(u))

	fmt.Print("User Password: ")
	password, err := terminal.ReadPassword(syscall.Stdin)
	if err != nil {
		return err
	}
	client, err := createClient(user, "")
	if err != nil {
		return err
	}
	resp, err := loginPassword(client.Client, user, string(password))
	if err != nil {
		return err
	}
	if c.printToken {
		if c.globalOpts.json {
			out, _ := json.Marshal(resp)
			fmt.Println(string(out))
		} else {
			fmt.Printf("%s|%s|%s\n", resp.UserID, resp.DeviceID, resp.AccessToken)
		}
	} else {
		if err := storeConfig(resp.UserID, resp.DeviceID, resp.AccessToken); err != nil {
			return err
		}
	}
	return nil
}
