package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"maunium.net/go/mautrix"
	"maunium.net/go/mautrix/id"
)

type globalOptions struct {
	roomID string
	userID string
	json   bool
	client *mautrix.Client
	config *config
}

func createClient(user id.UserID, token string) (*mautrix.Client, error) {
	_, homeserver, err := user.Parse()
	if err != nil {
		return nil, err
	}
	wellKnown, err := mautrix.DiscoverClientAPI(homeserver)
	if err != nil {
		return nil, err
	}
	client, err := mautrix.NewClient(wellKnown.Homeserver.BaseURL, user, token)
	if err != nil {
		return nil, err
	}
	return client, nil
}

func main() {
	var (
		globalOpts = globalOptions{}
		loginCmd   = loginCommand{globalOpts: &globalOpts}
		logoutCmd  = loginCommand{globalOpts: &globalOpts}
		roomCmd    = roomCommand{globalOpts: &globalOpts}
		sendCmd    = sendCommand{globalOpts: &globalOpts}
		syncCmd    = syncCommand{globalOpts: &globalOpts}
		userCmd    = userCommand{globalOpts: &globalOpts}
	)
	var (
		rootCobraCmd = &cobra.Command{
			Use:   "mnotify",
			Short: "mnotify",
			PersistentPreRun: func(cmd *cobra.Command, args []string) {
				// If we do a login, the config does not exist yet.
				// For all other commands, this is a fatal error.
				if cmd.CalledAs() == "login" {
					return
				}
				conf, err := loadConfig()
				if err != nil {
					cmd.PrintErrln(err)
					cmd.PrintErrln("create a valid login")
					os.Exit(1)
				}
				client, err := createClient(conf.UserID, conf.AccessToken)
				if err != nil {
					cmd.PrintErrln(err)
					os.Exit(1)
				}
				globalOpts.client = client
				globalOpts.config = &conf
			},
		}
		discoverCobraCmd = &cobra.Command{
			Use:   "discover",
			Short: "Perform a .well-known client discovery",
			RunE: func(cmd *cobra.Command, args []string) error {
				user := id.UserID(globalOpts.userID)
				_, homeserver, err := user.Parse()
				if err != nil {
					return err
				}
				wellKnown, err := mautrix.DiscoverClientAPI(homeserver)
				if err != nil {
					return err
				}
                // TODO: json needed?
				if wellKnown.Homeserver.BaseURL != "" {
                    fmt.Printf("Home Server: %s\n", wellKnown.Homeserver.BaseURL)
				}
				if wellKnown.IdentityServer.BaseURL != "" {
                    fmt.Printf("Indentity Server: %s\n", wellKnown.IdentityServer.BaseURL)
				}
				return nil
			},
		}
		loginCobraCmd = &cobra.Command{
			Use:   "login",
			Short: "Manage Login",
			RunE:  loginCmd.run,
		}
		logoutCobraCmd = &cobra.Command{
			Use:   "logout",
			Short: "Logout with this session",
			RunE:  logoutCmd.run,
		}
		roomCobraCmd = &cobra.Command{
			Use:   "room",
			Short: "Interact with matrix rooms (create, join, invite, …)",
			RunE:  roomCmd.run,
		}
		sendCobraCmd = &cobra.Command{
			Use:   "send",
			Short: "Send messages to a room",
			RunE:  sendCmd.run,
		}
		syncCobraCmd = &cobra.Command{
			Use:   "sync",
			Short: "Stream matrix event's to the terminal",
			RunE:  syncCmd.run,
		}
		userCobraCmd = &cobra.Command{
			Use:   "user",
			Short: "View and manage user data (avatar, display name, …)",
			RunE:  userCmd.run,
		}
		versionCobraCmd = &cobra.Command{
			Use:   "version",
			Short: "Ask the homeserver about supported protocol versions",
			RunE: func(cmd *cobra.Command, args []string) error {
				resp, err := globalOpts.client.Versions()
				if err != nil {
					return nil
				}
				cmd.Printf("Protocol Versions: %s\n", resp.Versions)
				cmd.Println("Unstable Features:")
				for k, v := range resp.UnstableFeatures {
					cmd.Printf("  %s: %v\n", k, v)
				}
				return nil
			},
		}
		whoamiCobraCmd = &cobra.Command{
			Use:   "whoami",
			Short: "Identify this login",
			RunE: func(cmd *cobra.Command, args []string) error {
				resp, err := globalOpts.client.Whoami()
				if err != nil {
					return nil
				}
				cmd.Printf("UserID  : %s\n", resp.UserID.String())
				if resp.DeviceID.String() != "" {
					cmd.Printf("DeviceID: %s\n", resp.DeviceID.String())
				}
				return nil
			},
		}
	)

	// globals
	globalFlags := rootCobraCmd.PersistentFlags()
	globalFlags.StringVarP(&globalOpts.userID, "user", "U", "", "Specify the full matrix user id")
	globalFlags.StringVarP(&globalOpts.roomID, "room", "R", "", "Specify a room to operate on")
	globalFlags.BoolVarP(&globalOpts.json, "json", "J", false, "Output JSON if supported")

	// discover
	rootCobraCmd.AddCommand(discoverCobraCmd)

	// login
	rootCobraCmd.AddCommand(loginCobraCmd)

	// logout
	rootCobraCmd.AddCommand(logoutCobraCmd)

	// room
	roomFlags := roomCobraCmd.Flags()
	roomFlags.BoolVarP(&roomCmd.create, "create", "c", false, "Create a new room")
	roomFlags.BoolVarP(&roomCmd.direct, "direct", "d", false, "Create a direct room")
	roomFlags.StringVarP(&roomCmd.profile, "profile", "", profileTrustedPrivate, fmt.Sprintf("The room profile [%s, %s, %s]", profileTrustedPrivate, profilePrivate, profilePublic))
	roomFlags.BoolVarP(&roomCmd.invite, "invite", "i", false, "Invite a user to a room")
	roomFlags.StringSliceVarP(&roomCmd.invites, "invites", "", []string{}, "A list of users to invite to a room")
	roomFlags.BoolVarP(&roomCmd.list, "list", "l", false, "List the user's rooms")
	roomFlags.BoolVarP(&roomCmd.leave, "leave", "", false, "Leave a room")
	roomFlags.BoolVarP(&roomCmd.forget, "forget", "", false, "Forget about a room")
	roomFlags.BoolVarP(&roomCmd.join, "join", "", false, "Join a room")
	roomFlags.BoolVarP(&roomCmd.messages, "messages", "m", false, "List messages of a room")
	roomFlags.UintVarP(&roomCmd.number, "number", "n", 10, "List messages of a room")
	rootCobraCmd.AddCommand(roomCobraCmd)

	// send
	sendFlags := sendCobraCmd.Flags()
	sendFlags.StringVarP(&sendCmd.message, "message", "m", "", "Send this message instead of stdin")
	rootCobraCmd.AddCommand(sendCobraCmd)

	// sync
	rootCobraCmd.AddCommand(syncCobraCmd)

	// user
	rootCobraCmd.AddCommand(userCobraCmd)

	// version
	rootCobraCmd.AddCommand(versionCobraCmd)

	// whomai
	rootCobraCmd.AddCommand(whoamiCobraCmd)

	// Wire everything up.
	rootCobraCmd.Execute()
}
