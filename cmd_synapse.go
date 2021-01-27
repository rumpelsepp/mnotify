package main

import (
	"fmt"
	"net/http"
	"os"

	"github.com/spf13/cobra"
	"maunium.net/go/mautrix"
)

type synapseCommand struct {
	globalOpts *globalOptions
}

func (c *synapseCommand) run(cmd *cobra.Command, args []string) error {
	return nil
}

type synapseVersionCommand struct {
	globalOpts *globalOptions
}

func (c *synapseVersionCommand) run(cmd *cobra.Command, args []string) error {
	var (
		client = c.globalOpts.client
		u      = client.BuildURL("server_version")
	)
	client.Prefix = mautrix.URLPath{"_synapse", "admin", "v1"}

	resp, err := client.MakeRequest("GET", u, nil, nil)
	if err != nil {
		return err
	}
	fmt.Println(string(resp))
	return nil
}

type synapseRoomCommand struct {
	globalOpts *globalOptions
	list       bool
	members    bool
}

func (c *synapseRoomCommand) run(cmd *cobra.Command, args []string) error {
	var (
		client = c.globalOpts.client
		u      string
	)
	client.Prefix = mautrix.URLPath{"_synapse", "admin", "v1"}

	switch {
	case c.list:
		u = client.BuildURL("rooms")
	case c.members:
		if c.globalOpts.roomID == "" {
			dieNoRoomID()
		}
		u = client.BuildURL("rooms", c.globalOpts.roomID, "members")
	default:
		fmt.Println("no argument given")
		os.Exit(1)
	}

	resp, err := client.MakeRequest(http.MethodGet, u, nil, nil)
	if err != nil {
		return err
	}
	fmt.Println(string(resp))
	return nil
}

type synapseUserCommand struct {
	globalOpts *globalOptions
	devices    bool
	show       bool
	whois      bool
}

func (c *synapseUserCommand) run(cmd *cobra.Command, args []string) error {
	var (
		client = c.globalOpts.client
		u      string
	)
	if c.globalOpts.userID == "" {
		// TODO: make helper
		fmt.Println("No user id given")
		os.Exit(1)
	}
	switch {
	case c.devices:
		client.Prefix = mautrix.URLPath{"_synapse", "admin", "v2"}
		u = client.BuildURL("users", c.globalOpts.userID, "devices")
	case c.show:
		client.Prefix = mautrix.URLPath{"_synapse", "admin", "v2"}
		u = client.BuildURL("users", c.globalOpts.userID)
	case c.whois:
		client.Prefix = mautrix.URLPath{"_synapse", "admin", "v1"}
		u = client.BuildURL("whois", c.globalOpts.userID)
	default:
		fmt.Println("no argument given")
		os.Exit(1)
	}
	resp, err := client.MakeRequest(http.MethodGet, u, nil, nil)
	if err != nil {
		return err
	}
	fmt.Println(string(resp))
	return nil
}

// type synapsePurgeHistory struct {
// 	globalOpts *globalOptions
// 	roomID     id.RoomID
// 	eventID    id.EventID
// 	status     bool
// }
//
// func (c *synapsePurgeHistory) run(cmd *cobra.Command, args []string) error {
// 	var (
// 		client = c.globalOpts.client
// 		u      string
// 	)
//
// 		client.Prefix = mautrix.URLPath{"_synapse", "admin", "v1"}
// 		u = client.BuildURL("purge_history", c.globalOpts.userID)
// 		u = client.BuildURL("purge_history_status", c.globalOpts.userID)
// }
