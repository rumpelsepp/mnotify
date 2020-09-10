package main

import (
	"encoding/json"
	"fmt"

	"github.com/spf13/cobra"
	"maunium.net/go/mautrix/id"
)

type userCommand struct {
	globalOpts *globalOptions
}

type outData struct {
	UserID      string `json:"user_id"`
	DisplayName string `json:"display_name,omitempty"`
}

func (c *userCommand) run(cmd *cobra.Command, args []string) error {
	var (
		client = c.globalOpts.client
		user   id.UserID
	)
	if c.globalOpts.userID != "" {
		user = id.UserID(c.globalOpts.userID)
	} else if c.globalOpts.config.UserID != "" {
		user = id.UserID(c.globalOpts.config.UserID)
	} else {
		r, err := client.Whoami()
		if err != nil {
			return err
		}
		user = r.UserID
	}
	// TODO: Implement GetProfile() in mautrix
	respDisplayName, err := client.GetDisplayName(user)
	if err != nil {
		return err
	}
	// FIXME: The answer here is empty. Why?
	// _, err = client.GetAvatarURL(user)
	// if err != nil {
	// 	return err
	// }
	out := outData{
		UserID:      user.String(),
		DisplayName: respDisplayName.DisplayName,
	}
	if c.globalOpts.json {
		o, _ := json.Marshal(out)
		fmt.Println(string(o))
	} else {
		fmt.Printf("%s|%s\n", out.UserID, out.DisplayName)
	}
	return nil
}
