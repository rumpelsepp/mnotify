package main

import (
	"io/ioutil"
	"os"

	"github.com/spf13/cobra"
	"maunium.net/go/mautrix/event"
	"maunium.net/go/mautrix/id"
)

type sendCommand struct {
	globalOpts *globalOptions
	message    string
	formatted  string
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
		e := event.MessageEventContent{
			MsgType:       event.MsgText,
			Body:          msg,
			Format:        event.FormatHTML,
			FormattedBody: c.formatted,
		}
		_, err = c.globalOpts.client.SendMessageEvent(id.RoomID(c.globalOpts.roomID), event.EventMessage, e)
	} else {
		_, err = c.globalOpts.client.SendText(id.RoomID(c.globalOpts.roomID), msg)
	}
	if err != nil {
		return err
	}
	return nil
}
