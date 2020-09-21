# mnotify

`mnotify` is a simple cli for the [matrix](https://matrix.org) chat system.
It was developed for the use case of sending notifications from a headless server.
It seems to develop into a fully fledged CLI client usable from oldfashioned scripts.

## Build

```
$ make
```

## Contribute

There is a [mailing list](https://lists.sr.ht/~rumpelsepp/public-inbox).
Instructions about sending patches are at this [tutorial webpage](https://git-send-email.io/).
Just use the builtin functionality of git. :)

## Get Started

Obtain a fresh matrix user account on an arbitrary homeserver.
First, create a login.

`mnotify` asks interactively for the username and the password.
Be aware to **always** use the complete matrix id including the domain, e.g. `@user:example.org`.

```
$ mnotify login
```

The access token is stored in `~/.config/mnotify/config.toml`.
Keep this file secret.

Now create a room and invite the user to whom you want to send notifications (here: `@user:example.org`).

```
$ mnotify room --create --direct --invites "@user:example.org"
```

Once the user `@user:example.org` joins the room, text messages can be sent to the room like this:

```
$ echo "Hello World!" | mnotify send --room !gBSqYoCSkyAHgqJEcW:hackbrettl.de
```

Alternatively it can be used like this:

```
$ mnotify send --room !gBSqYoCSkyAHgqJEcW:hackbrettl.de --message "Hello World!"
```

## Further Examples

The output format is column based, the column separator is always `|` enabling `awk` magic.
Every command that produces output understands the `-J` or `--json` switch.
On each line one JSON object is printed.

### Stream Messages

```
$ mnotify sync
Sep  9 15:23:28.987|m.room.message|$sBZUatMPSqnfqYaD1W-4g4wsHu8vq08wGPD-xYTn6mk|!JJaziyTEiTFWEEWILE:hackbrettl.de|@develop:hackbrettl.de|test
Sep  9 23:23:48.641|m.room.message|$0kQhv_iyRwL8Us4nbDbij19JkC9A9Nl3dtZPhhVj2MI|!JJaziyTEiTFWEEWILE:hackbrettl.de|@rumpelsepp:hackbrettl.de|test
Sep  9 23:23:51.107|m.room.message|$3j4G4eD3ASni9rmxrT2qQV-vqCa_l1L4x9eP80n4818|!JJaziyTEiTFWEEWILE:hackbrettl.de|@rumpelsepp:hackbrettl.de|hello
```

```
$ mnotify sync --json | jq
{
  "body": "test",
  "event_id": "$sBZUatMPSqnfqYaD1W-4g4wsHu8vq08wGPD-xYTn6mk",
  "event_type": "m.room.message",
  "room_id": "!JJaziyTEiTFWEEWILE:hackbrettl.de",
  "sender": "@develop:hackbrettl.de",
  "timestamp": "2020-09-09T15:23:28.987+02:00"
}
```

### List Rooms

```
$ mnotify room -l --json
{
  "room_id": "!JJaziyTEiTFWEEWILE:hackbrettl.de",
  "members": [
    {
      "user_id": "@develop:hackbrettl.de",
      "display_name": "develop"
    },
    {
      "user_id": "@rumpelsepp:hackbrettl.de",
      "display_name": "stefan"
    }
  ]
}
```
