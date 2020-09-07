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
$ mnotify login
```

The access token is stored in `~/.config/mnotify/config.toml`.
Keep this file secret.

Now create a room and invite the user to whom you want to send notifications (here: `@user:example.org`).

```
$ mnotify room --create
!gBSqYoCSkyAHgqJEcW:hackbrettl.de
$ mnotify room --invive --user "@user:example.org" --room "!gBSqYoCSkyAHgqJEcW:hackbrettl.de"
```

Once the user `@user:example.org` joins the room, text messages can be sent to the room like this:

```
$ echo "Hello World!" | mnotify send --room !gBSqYoCSkyAHgqJEcW:hackbrettl.de
```

Alternatively it can be used like this:

```
$ mnotify send --room !gBSqYoCSkyAHgqJEcW:hackbrettl.de --message "Hello World!"
```

## Build

```
$ make
```
