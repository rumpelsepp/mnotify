package main

import (
	"fmt"

	"maunium.net/go/mautrix"
	"maunium.net/go/mautrix/event"
	"maunium.net/go/mautrix/id"
)

type StateCache struct {
	client *mautrix.Client
}

func NewStateCache(client *mautrix.Client) (*StateCache, error) {
	return &StateCache{
		client: client,
	}, nil
}

func (c *StateCache) IsEncrypted(roomID id.RoomID) bool {
	var evt event.Event
	err := c.client.StateEvent(roomID, event.StateEncryption, "", &evt)
	if err != nil {
		return false
	}
	return true
}

func (c *StateCache) GetEncryptionEvent(roomID id.RoomID) *event.EncryptionEventContent {
	var (
		room = mautrix.NewRoom(roomID)
		evt  = room.GetStateEvent(event.StateEncryption, "")
	)
	if evt == nil {
		return nil
	}
	content, ok := evt.Content.Parsed.(*event.EncryptionEventContent)
	if !ok {
		return nil
	}
	return content
}

func (c *StateCache) FindSharedRooms(userID id.UserID) []id.RoomID {
	var out []id.RoomID
	joined, err := c.client.JoinedRooms()
	if err != nil {
		fmt.Println(err)
		return out
	}
	for _, roomID := range joined.JoinedRooms {
		room := mautrix.NewRoom(roomID)
		if !c.IsEncrypted(roomID) {
			continue
		}
		evt := room.GetMembershipState(userID)
		if evt == event.MembershipJoin {
			out = append(out, room.ID)
		}
	}
	return out
}
