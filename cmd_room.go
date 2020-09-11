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
	globalOpts     *globalOptions
	create         bool
	direct         bool
	invites        []string
	invite         bool
	list           bool
	leave          bool
	forget         bool
	join           bool
	messages       bool
	includeMembers bool
	number         uint
	profile        string
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

	var (
		client  = c.globalOpts.client
		roomID  = id.RoomID(c.globalOpts.roomID)
		invites []id.UserID
	)
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
		resp, err := client.CreateRoom(req)
		if err != nil {
			return err
		}
		fmt.Println(resp.RoomID)
	case c.invite:
		for _, user := range invites {
			req := &mautrix.ReqInviteUser{
				UserID: user,
			}
			_, err := client.InviteUser(roomID, req)
			if err != nil {
				return err
			}
		}
	case c.list:
		rooms, err := client.JoinedRooms()
		if err != nil {
			return err
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
			if c.includeMembers {
				members, err := client.JoinedMembers(room.ID)
				if err != nil {
					fmt.Printf("error room %s: %s\n", roomID, err)
					continue
				}
				for k, v := range members.Joined {
					var displayName string
					if v.DisplayName != nil {
						displayName = *v.DisplayName
					}
					m := member{
						UserID:      string(k),
						DisplayName: displayName,
					}
					out.Members = append(out.Members, m)
				}
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
		if roomID == "" {
			dieNoRoomID()
		}
		_, err := client.JoinRoomByID(roomID)
		if err != nil {
			return err
		}
	case c.leave:
		if roomID == "" {
			dieNoRoomID()
		}
		_, err := client.LeaveRoom(id.RoomID(c.globalOpts.roomID))
		if err != nil {
			return err
		}
	case c.messages:
		if roomID == "" {
			dieNoRoomID()
		}
		fmt.Println("This subcommand is currently broken")
		resp, err := client.Messages(roomID, "", "", 'f', int(c.number))
		if err != nil {
			return err
		}
		// FIXME: This response is always empty. What am I doing wrong?
		fmt.Printf("%+v\n", resp)
	case c.forget:
		if roomID == "" {
			dieNoRoomID()
		}
		_, err := client.ForgetRoom(id.RoomID(c.globalOpts.roomID))
		if err != nil {
			return err
		}
	default:
	}
	return nil
}
