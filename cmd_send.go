package main

import (
	"io/ioutil"
	"os"

	"github.com/spf13/cobra"
	"maunium.net/go/mautrix/id"
	"maunium.net/go/mautrix/event"
)

type sendCommand struct {
	globalOpts *globalOptions
	message    string
	formatted  string
	    string
}

func (c *sendCommand) run(cmd *cobra.Command, args []string) error {
	var (
		err error
		msg string
	)
	if c.message != "" {
		msg = c.message
	} else {
		m, err := ioutil.ReadAll(os.Stdin)
		if err != nil {
			return err
		}
		if len(m) == 0 {
			os.Exit(0)
		}
		msg = string(m)
	}

	if c.formatted != "" {
		var f = event.MessageEventContent{MsgType: "m.text", Body: msg, Format: "org.matrix.custom.html", FormattedBody: c.formatted}
		_, err = c.globalOpts.client.SendMessageEvent(id.RoomID(c.globalOpts.roomID), event.Type{"m.room.message", event.MessageEventType}, f)
	} else {
		_, err = c.globalOpts.client.SendText(id.RoomID(c.globalOpts.roomID), msg)
	}
	if err != nil {
		return err
	}
	return nil
}
