package main

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"os"
	"strconv"

	"github.com/spf13/cobra"
	"maunium.net/go/mautrix"
	"maunium.net/go/mautrix/crypto"
	"maunium.net/go/mautrix/event"
	"maunium.net/go/mautrix/id"
	_ "modernc.org/sqlite"
)

type globalOptions struct {
	roomID string
	userID string
	json   bool

	client *mnotifyClient
	config *config
}

type mnotifyClient struct {
	*mautrix.Client
	olmMachine *crypto.OlmMachine
	store      *crypto.SQLCryptoStore
	statecache *StateCache
}

func createClient(user id.UserID, token string) (*mnotifyClient, error) {
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

	db, err := sql.Open("sqlite", "file:///"+stateDir())
	if err != nil {
		panic(err)
	}
	l := os.Getenv("MNOTIFY_LOG")
	logLevel := 0
	if l != "" {
		if val, err := strconv.Atoi(l); err != nil {
			logLevel = val
		}
	}

	logger := cryptoLogger{level: logLevel}
	store := crypto.NewSQLCryptoStore(db, "sqlite3", client.DeviceID.String(), client.DeviceID, []byte(client.DeviceID.String()+"pickle"), &logger)
	// Try to create the tables if they are missing
	if err = store.CreateTables(); err != nil {
		return nil, err
	}
	statecache, err := NewStateCache(client)
	if err != nil {
		return nil, err
	}
	olmMachine := crypto.NewOlmMachine(client, &logger, store, statecache)
	if err := olmMachine.Load(); err != nil {
		return nil, err
	}

	mclient := &mnotifyClient{
		Client:     client,
		olmMachine: olmMachine,
		store:      store,
		statecache: statecache,
	}
	mclient.Client.Syncer.(*mautrix.DefaultSyncer).OnSync(mclient.syncCallback)
	mclient.Client.Syncer.(*mautrix.DefaultSyncer).OnEventType(event.StateMember, func(source mautrix.EventSource, evt *event.Event) {
		olmMachine.HandleMemberEvent(evt)
	})

	return mclient, nil
}

func (c *mnotifyClient) syncCallback(resp *mautrix.RespSync, since string) bool {
	c.olmMachine.ProcessSyncResponse(resp, since)
	if err := c.olmMachine.CryptoStore.Flush(); err != nil {
		fmt.Println(err)
	}
	return true
}

type cryptoLogger struct {
	level int
}

func (l *cryptoLogger) Error(message string, args ...interface{}) {
	if l.level > 0 {
		fmt.Printf("error: "+message, args...)
	}
}

func (l *cryptoLogger) Warn(message string, args ...interface{}) {
	if l.level > 1 {
		fmt.Printf("warn: "+message, args...)
	}
}

func (l *cryptoLogger) Debug(message string, args ...interface{}) {
	if l.level > 2 {
		fmt.Printf("debug: "+message, args...)
	}
}

func (l *cryptoLogger) Trace(message string, args ...interface{}) {
	if l.level > 3 {
		fmt.Printf("trace: "+message, args...)
	}
}

func main() {
	var (
		globalOpts        = globalOptions{}
		loginCmd          = loginCommand{globalOpts: &globalOpts}
		logoutCmd         = logoutCommand{globalOpts: &globalOpts}
		roomCmd           = roomCommand{globalOpts: &globalOpts}
		sendCmd           = sendCommand{globalOpts: &globalOpts}
		synapseCmd        = synapseCommand{globalOpts: &globalOpts}
		synapseRoomCmd    = synapseRoomCommand{globalOpts: &globalOpts}
		synapseUserCmd    = synapseUserCommand{globalOpts: &globalOpts}
		synapseVersionCmd = synapseVersionCommand{globalOpts: &globalOpts}
		syncCmd           = syncCommand{globalOpts: &globalOpts}
		userCmd           = userCommand{globalOpts: &globalOpts}
	)

	conf, confErr := loadConfig()
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
				if confErr != nil {
					cmd.PrintErrln(confErr)
					cmd.PrintErrln("create a valid login")
					os.Exit(1)
				}
				client, err := createClient(conf.UserID, conf.AccessToken)
				if err != nil {
					cmd.PrintErrln(err)
					os.Exit(1)
				}
				globalOpts.client = client
				globalOpts.config = conf
			},
			SilenceUsage: true,
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
				if globalOpts.json {
					o, _ := json.Marshal(wellKnown)
					fmt.Println(string(o))
					return nil
				}
				if wellKnown.Homeserver.BaseURL != "" {
					fmt.Printf("Home Server: %s\n", wellKnown.Homeserver.BaseURL)
				}
				if wellKnown.IdentityServer.BaseURL != "" {
					fmt.Printf("Identity Server: %s\n", wellKnown.IdentityServer.BaseURL)
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
			Short: "Interact with matrix rooms (create, join, invite, ???)",
			RunE:  roomCmd.run,
		}
		sendCobraCmd = &cobra.Command{
			Use:   "send",
			Short: "Send messages to a room",
			RunE:  sendCmd.run,
		}
		synapseCobraCmd = &cobra.Command{
			Use:   "synapse",
			Short: "Use the synapse admin api",
			RunE:  synapseCmd.run,
		}
		synapseRoomCobraCmd = &cobra.Command{
			Use:   "room",
			Short: "Administrate rooms",
			RunE:  synapseRoomCmd.run,
		}
		synapseUserCobraCmd = &cobra.Command{
			Use:   "user",
			Short: "Administrate users",
			RunE:  synapseUserCmd.run,
		}
		synapseVersionCobraCmd = &cobra.Command{
			Use:   "version",
			Short: "Query synapse version",
			RunE:  synapseVersionCmd.run,
		}
		syncCobraCmd = &cobra.Command{
			Use:   "sync",
			Short: "Stream matrix events to the terminal",
			RunE:  syncCmd.run,
		}
		userCobraCmd = &cobra.Command{
			Use:   "user",
			Short: "View and manage user data (avatar, display name, ???)",
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

	roomDefault := ""
	if conf != nil {
		roomDefault = string(conf.DefaultRoom)
	}

	// globals
	globalFlags := rootCobraCmd.PersistentFlags()
	globalFlags.StringVarP(&globalOpts.userID, "user", "U", "", "Specify the full matrix user id")
	globalFlags.StringVarP(&globalOpts.roomID, "room", "R", roomDefault, "Specify a room to operate on")
	globalFlags.BoolVarP(&globalOpts.json, "json", "J", false, "Output JSON if supported")

	// discover
	rootCobraCmd.AddCommand(discoverCobraCmd)

	// login
	rootCobraCmd.AddCommand(loginCobraCmd)

	// logout
	rootCobraCmd.AddCommand(logoutCobraCmd)
	logoutFlags := logoutCobraCmd.Flags()
	logoutFlags.BoolVarP(&logoutCmd.force, "force", "f", false, "Perform the logout")

	// room
	roomFlags := roomCobraCmd.Flags()
	roomFlags.BoolVarP(&roomCmd.create, "create", "c", false, "Create a new room")
	roomFlags.BoolVarP(&roomCmd.direct, "direct", "d", false, "Create a direct room")
	roomFlags.StringVarP(&roomCmd.profile, "profile", "", profileTrustedPrivate, fmt.Sprintf("The room profile [%s, %s, %s]", profileTrustedPrivate, profilePrivate, profilePublic))
	roomFlags.BoolVarP(&roomCmd.invite, "invite", "i", false, "Invite a user to a room")
	roomFlags.StringSliceVarP(&roomCmd.invites, "invites", "", []string{}, "A list of users to invite to a room")
	roomFlags.BoolVarP(&roomCmd.includeMembers, "members", "", false, "Include room members")
	roomFlags.BoolVarP(&roomCmd.list, "list", "l", false, "List the user's rooms")
	roomFlags.BoolVarP(&roomCmd.leave, "leave", "", false, "Leave a room")
	roomFlags.BoolVarP(&roomCmd.forget, "forget", "", false, "Forget about a room")
	roomFlags.BoolVarP(&roomCmd.join, "join", "", false, "Join a room")
	roomFlags.BoolVarP(&roomCmd.messages, "messages", "m", false, "List messages of a room")
	roomFlags.UintVarP(&roomCmd.number, "number", "n", 10, "List messages of a room")
	roomFlags.StringVarP(&roomCmd.state, "state", "", "", "Output the value of a state/type key")
	rootCobraCmd.AddCommand(roomCobraCmd)

	// send
	sendFlags := sendCobraCmd.Flags()
	sendFlags.StringVarP(&sendCmd.message, "message", "m", "", "Send this message instead of stdin")
	sendFlags.StringVarP(&sendCmd.formatted, "formatted", "", "", "Alternative formatted text")
	sendFlags.BoolVarP(&sendCmd.markdown, "markdown", "d", false, "Render the message as markdown")
	rootCobraCmd.AddCommand(sendCobraCmd)

	// synapse
	rootCobraCmd.AddCommand(synapseCobraCmd)
	synapseCobraCmd.AddCommand(synapseRoomCobraCmd)
	synapseRoomFlags := synapseRoomCobraCmd.Flags()
	synapseRoomFlags.BoolVarP(&synapseRoomCmd.list, "list", "l", false, "List all rooms on the server")
	synapseRoomFlags.BoolVarP(&synapseRoomCmd.members, "members", "m", false, "List members of a room")
	synapseCobraCmd.AddCommand(synapseUserCobraCmd)
	synapseUserFlags := synapseUserCobraCmd.Flags()
	synapseUserFlags.BoolVarP(&synapseUserCmd.devices, "devices", "d", false, "List the user's devices")
	synapseUserFlags.BoolVarP(&synapseUserCmd.show, "show", "s", false, "Show the data associated with the user")
	synapseUserFlags.BoolVarP(&synapseUserCmd.whois, "whois", "w", false, "List current logins")
	synapseCobraCmd.AddCommand(synapseVersionCobraCmd)

	// sync
	rootCobraCmd.AddCommand(syncCobraCmd)
	syncFlags := syncCobraCmd.Flags()
	syncFlags.BoolVarP(&syncCmd.once, "once", "", false, "Just sync once (without this, will sync forever)")
	syncFlags.BoolVarP(&syncCmd.presence, "presence", "p", false, "Set presence to online")
	syncFlags.IntVarP(&syncCmd.syncTimeout, "timeout", "t", 30000, "Matrix sync timeout in ms")

	// user
	rootCobraCmd.AddCommand(userCobraCmd)

	// version
	rootCobraCmd.AddCommand(versionCobraCmd)

	// whoami
	rootCobraCmd.AddCommand(whoamiCobraCmd)

	// Wire everything up.
	rootCobraCmd.Execute()
}
