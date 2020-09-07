package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"maunium.net/go/mautrix"
	"maunium.net/go/mautrix/event"
	"maunium.net/go/mautrix/id"
)

type roomCommand struct {
	globalOpts *globalOptions
	create     bool
	invite     bool
	list       bool
}

func (c *roomCommand) run(cmd *cobra.Command, args []string) error {
	switch {
	case c.create:
		req := &mautrix.ReqCreateRoom{
			Preset: "trusted_private_chat",
		}
		resp, err := c.globalOpts.client.CreateRoom(req)
		if err != nil {
			return err
		}
		fmt.Println(resp.RoomID)
	case c.invite:
		req := &mautrix.ReqInviteUser{
			UserID: id.UserID(c.globalOpts.userID),
		}
		_, err := c.globalOpts.client.InviteUser(id.RoomID(c.globalOpts.roomID), req)
		if err != nil {
			return err
		}
	case c.list:
		rooms, err := c.globalOpts.client.JoinedRooms()
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
				members, err := c.globalOpts.client.JoinedMembers(room.ID)
				if err != nil {
					fmt.Printf("error room %s: %s\n", rawRoom, err)
					continue
				}
				fmt.Println(rawRoom)
				for k, v := range members.Joined {
					fmt.Printf("  %s (%s)\n", *v.DisplayName, k)
				}
				fmt.Println("")
			}
		}
	}
	return nil
}
