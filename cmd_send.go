package main

import (
	"io/ioutil"
	"os"

	"github.com/spf13/cobra"
	"maunium.net/go/mautrix"
	"maunium.net/go/mautrix/crypto"
	"maunium.net/go/mautrix/event"
	"maunium.net/go/mautrix/format"
	"maunium.net/go/mautrix/id"
)

type sendCommand struct {
	globalOpts *globalOptions
	message    string
	formatted  string
	markdown   bool
}

func sendEvent(client *mnotifyClient, evt event.Event) (*mautrix.RespSendEvent, error) {
	if client.statecache.IsEncrypted(evt.RoomID) {
		joinedMap, err := client.Client.JoinedMembers(evt.RoomID)
		if err != nil {
			return nil, err
		}
		var joined []id.UserID
		for k := range joinedMap.Joined {
			joined = append(joined, k)
		}
		encrypted, err := client.olmMachine.EncryptMegolmEvent(evt.RoomID, event.EventMessage, &evt.Content)
		// These three errors mean we have to make a new Megolm session
		if err == crypto.SessionExpired || err == crypto.SessionNotShared || err == crypto.NoGroupSession {
			if err = client.olmMachine.ShareGroupSession(evt.RoomID, joined); err != nil {
				return nil, err
			}
			encrypted, err = client.olmMachine.EncryptMegolmEvent(evt.RoomID, evt.Type, &evt.Content)
		}
		if err != nil {
			return nil, err
		}
		evt.Type = event.EventEncrypted
		evt.Content = event.Content{Parsed: encrypted}
	}

	req := mautrix.ReqSendEvent{TransactionID: evt.Unsigned.TransactionID}
	resp, err := client.Client.SendMessageEvent(evt.RoomID, evt.Type, &evt.Content, req)
	if err != nil {
		return nil, err
	}
	return resp, nil
}

func (c *sendCommand) run(cmd *cobra.Command, args []string) error {
	var (
		err    error
		msg    string
		client = c.globalOpts.client
		roomID = id.RoomID(c.globalOpts.roomID)
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

	sendEvtContent := event.MessageEventContent{
		MsgType:       event.MsgText,
		Body:          msg,
		Format:        event.FormatHTML,
		FormattedBody: c.formatted,
	}

	if c.formatted != "" {
		sendEvtContent.Body = msg
		sendEvtContent.Format = event.FormatHTML
		sendEvtContent.FormattedBody = c.formatted
	} else if c.markdown {
		sendEvtContent = format.RenderMarkdown(msg, true, true)
	} else {
		sendEvtContent.Body = msg
	}

	sendEvt := event.Event{
		Type:    event.EventMessage,
		RoomID:  roomID,
		Content: event.Content{Parsed: sendEvtContent},
	}
	_, err = sendEvent(client, sendEvt)
	if err != nil {
		return err
	}
	return nil
}
