package main

import (
	"encoding/json"
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
	type member struct {
		UserID      string `json:"user_id"`
		DisplayName string `json:"display_name,omitempty"`
	}
	type outData struct {
		RoomID   string   `json:"room_id"`
		RoomName string   `json:"room_name,omitempty"`
		Members  []member `json:"members,omitempty"`
	}

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
		for _, roomID := range rooms.JoinedRooms {
			var (
				room  = mautrix.NewRoom(roomID)
				event = room.GetStateEvent(event.StateCanonicalAlias, "")
				out   = outData{
					RoomID: string(roomID),
				}
			)
			if event != nil {
				out.RoomName = string(event.Content.AsCanonicalAlias().Alias)
			}
			members, err := c.globalOpts.client.JoinedMembers(room.ID)
			if err != nil {
				fmt.Printf("error room %s: %s\n", roomID, err)
				continue
			}
			for k, v := range members.Joined {
				m := member{
					UserID:      string(k),
					DisplayName: *v.DisplayName,
				}
				out.Members = append(out.Members, m)
			}
			if c.globalOpts.json {
				o, _ := json.Marshal(out)
				fmt.Println(string(o))
			} else {
				fmt.Println(out.RoomID)
				for _, m := range out.Members {
					fmt.Printf("  %s|%s\n", m.DisplayName, m.UserID)
				}
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
