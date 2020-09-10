package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

type logoutCommand struct {
	globalOpts *globalOptions
	force      bool
}

func (c *logoutCommand) run(cmd *cobra.Command, args []string) error {
	if c.force {
		_, err := c.globalOpts.client.Logout()
		return err
	}
	fmt.Println("If you really want to logout, enforce it with `-f`")
	return nil
}
