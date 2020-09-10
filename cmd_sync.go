package main

import (
	"encoding/json"
	"fmt"
	"time"

	"github.com/spf13/cobra"
	"maunium.net/go/mautrix"
	"maunium.net/go/mautrix/event"
	"maunium.net/go/mautrix/id"
)

type syncCommand struct {
	globalOpts  *globalOptions
	presence    bool
	syncTimeout int
}

func (c *syncCommand) printEventJSON(roomID id.RoomID, e *event.Event) {
	type outData struct {
		Body      string    `json:"body,omitempty"`
		Error     string    `json:"error,omitempty"`
		EventID   string    `json:"event_id"`
		EventType string    `json:"event_type"`
		RoomID    string    `json:"room_id,omitempty"`
		Sender    string    `json:"sender,omitempty"`
		Timestamp time.Time `json:"timestamp"`
	}
	var (
		s        = e.Timestamp / 1000
		ns       = (e.Timestamp - s*1000) * 1000000
		outEvent = outData{
			EventID:   e.ID.String(),
			EventType: e.Type.String(),
			RoomID:    roomID.String(),
			Sender:    e.Sender.String(),
			Timestamp: time.Unix(s, ns),
		}
	)
	switch e.Type {
	case event.EventMessage:
		if val, ok := e.Content.Raw["body"]; ok {
			outEvent.Body = fmt.Sprintf("%s", val)
		}
	default:
		outEvent.Error = fmt.Sprintf("event type %s not implemented", e.Type.String())
	}

	if c.globalOpts.json {
		out, _ := json.Marshal(outEvent)
		fmt.Println(string(out))
	} else {
		if outEvent.Error != "" {
			fmt.Printf(
				"!%s|%s|%s|%s|%s|%s\n",
				outEvent.Timestamp.Format(time.StampMilli),
				outEvent.EventType,
				outEvent.EventID,
				outEvent.RoomID,
				outEvent.Sender,
				outEvent.Error,
			)
		} else {
			fmt.Printf(
				"%s|%s|%s|%s|%s|%s\n",
				outEvent.Timestamp.Format(time.StampMilli),
				outEvent.EventType,
				outEvent.EventID,
				outEvent.RoomID,
				outEvent.Sender,
				outEvent.Body,
			)
		}
	}
}

func (c *syncCommand) printSync(s *mautrix.RespSync) {
	if len(s.Rooms.Join) > 0 {
		for roomID, roomEvent := range s.Rooms.Join {
			for _, event_ := range roomEvent.Timeline.Events {
				c.printEventJSON(roomID, event_)
			}
		}
	}
}

func (c *syncCommand) run(cmd *cobra.Command, args []string) error {
	var (
		err       error
		resp      *mautrix.RespSync
		client    = c.globalOpts.client
		nextBatch = ""
		filterID  = ""
		// TODO: Bug in mautrix; presence type not set on constant.
		presence event.Presence = event.PresenceOffline
	)

	for {
		if c.globalOpts.roomID != "" {
			filter := &mautrix.Filter{
				Room: mautrix.RoomFilter{
					Rooms: []id.RoomID{id.RoomID(c.globalOpts.roomID)},
				},
			}
			resp, err := client.CreateFilter(filter)
			if err != nil {
				return err
			}
			filterID = resp.FilterID
		}
		if c.presence {
			presence = event.PresenceOnline
		}
		resp, err = client.SyncRequest(c.syncTimeout, nextBatch, filterID, false, presence)
		if err != nil {
			return err
		}

		nextBatch = resp.NextBatch
		c.printSync(resp)
	}
}
