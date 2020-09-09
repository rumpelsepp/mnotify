package main

import (
	"github.com/spf13/cobra"
)

type logoutCommand struct {
	globalOpts *globalOptions
}

func (c *logoutCommand) run(cmd *cobra.Command, args []string) error {
	_, err := c.globalOpts.client.Logout()
	return err
}
