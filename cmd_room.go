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
	direct     bool
	invites    []string
	invite     bool
	list       bool
	leave      bool
	forget     bool
	join       bool
	profile    string
}

const (
	profilePrivate        = "private_chat"
	profileTrustedPrivate = "trusted_private_chat"
	profilePublic         = "public_chat"
)

func dieNoRoomID() {
	fmt.Println("no operation supplied")
	os.Exit(1)
}

func (c *roomCommand) run(cmd *cobra.Command, args []string) error {
	var invites []id.UserID
	for _, user := range c.invites {
		invites = append(invites, id.UserID(user))
	}
	switch {
	case c.create:
		req := &mautrix.ReqCreateRoom{
			Preset:   c.profile,
			IsDirect: c.direct,
			Invite:   invites,
		}
		resp, err := c.globalOpts.client.CreateRoom(req)
		if err != nil {
			return err
		}
		fmt.Println(resp.RoomID)
	case c.invite:
		for _, user := range invites {
			req := &mautrix.ReqInviteUser{
				UserID: user,
			}
			_, err := c.globalOpts.client.InviteUser(id.RoomID(c.globalOpts.roomID), req)
			if err != nil {
				return err
			}
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
	case c.join:
		if c.globalOpts.roomID == "" {
			dieNoRoomID()
		}
		_, err := c.globalOpts.client.JoinRoomByID(id.RoomID(c.globalOpts.roomID))
		if err != nil {
			return err
		}
	case c.leave:
		if c.globalOpts.roomID == "" {
			dieNoRoomID()
		}
		_, err := c.globalOpts.client.LeaveRoom(id.RoomID(c.globalOpts.roomID))
		if err != nil {
			return err
		}
	case c.forget:
		if c.globalOpts.roomID == "" {
			dieNoRoomID()
		}
		_, err := c.globalOpts.client.ForgetRoom(id.RoomID(c.globalOpts.roomID))
		if err != nil {
			return err
		}
	default:
	}
	return nil
}