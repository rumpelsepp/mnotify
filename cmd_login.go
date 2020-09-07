package main

import (
	"bufio"
	"fmt"
	"os"
	"strings"
	"syscall"

	"github.com/spf13/cobra"
	"golang.org/x/crypto/ssh/terminal"
	"maunium.net/go/mautrix"
	"maunium.net/go/mautrix/id"
)

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
	token, err := loginPassword(client, user, string(password))
	if err != nil {
		return err
	}
	if c.printToken {
		fmt.Println(user)
	} else {
		if err := storeConfig(user, token); err != nil {
			return err
		}
	}
	return nil
}
