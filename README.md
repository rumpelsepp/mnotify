# mnotify

`mnotify` is a simple cli for the [matrix](https://matrix.org) chat system.
It was developed for the use case of sending notifications from a headless server.
The binary is called `mn`.

## Build

```
$ cargo build
```

## Get Started

Obtain a fresh matrix user account on an arbitrary homeserver.
First, create a login.

Be aware to **always** use the complete matrix id including the domain, e.g. `@user:example.org`.

```
$ mn login @user:example.org
```

The access token is stored in the system keyring.
If you are on a remote machine without a keyring daemon, use the env variable `MN_NO_KEYRING`;
in this case the sync token will be stored in a file `$XDG_STATE_HOME/mn/session.json`.