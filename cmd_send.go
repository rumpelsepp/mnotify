package main

import (
	"io/ioutil"
	"os"

	"github.com/spf13/cobra"
	"maunium.net/go/mautrix/id"
)

type sendCommand struct {
	globalOpts *globalOptions
	message    string
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

	_, err = c.globalOpts.client.SendText(id.RoomID(c.globalOpts.roomID), msg)
	if err != nil {
		return err
	}
	return nil
}
