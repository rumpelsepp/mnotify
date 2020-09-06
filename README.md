# mnotify

`mnotify` is a simple cli for the [matrix](https://matrix.org) chat system.
It was developed for the use case of sending notifications from a headless server.
It might eventually become a more powerful cli matrix client as initially expected…

## Supported Features

* Login via password, the homeserver is automatically discovered
* Sending text messages
* Create private rooms
* Invite users to a room

## Get Started

Obtain a fresh matrix user account on an arbitrary homeserver.
First, create a login.

`mnotify` asks interactively for the username and the password.
Be aware to **always** use the complete matrix id including the domain, e.g. `@user:example.org`.

```
$ mnotify -l
```

The access token is stored in `~/.config/mnotify/config.toml`.
Keep this file secret.

Now create a room and invite the user to whom you want to send notifications (here: `@user:example.org`).

```
$ mnotify -c
!gBSqYoCSkyAHgqJEcW:hackbrettl.de
$ mnotify -u "@user:example.org" -r "!gBSqYoCSkyAHgqJEcW:hackbrettl.de" -i
```

Once the user `@user:example.org` joins the room, text messages can be sent to the room like this:

```
$ echo "Hello World!" | mnotify -r !gBSqYoCSkyAHgqJEcW:hackbrettl.de
```

## Future Work

In case we add more features to `mnotify`, the cli must be restructured.
For now it is okay.

## Build

```
$ make
```
