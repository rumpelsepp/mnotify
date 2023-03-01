# mnotify

ATTENTION: Currently under development; expect breakage.

`mnotify` is a simple cli for the [matrix](https://matrix.org) chat system.
It was developed for the use case of sending notifications from a headless server.
The binary is called `mn`.
The output is always JSON.

## Build

```
$ cargo build
```

## Get Started

Obtain a fresh matrix user account on an arbitrary homeserver.

### Login (Password)

First, create a login.

Be aware to **always** use the complete matrix id including the domain, e.g. `@user:example.org`.
Without the `-p` flag, `mn` reads the password from stdin or interactively from the terminal.

```
$ mn login @user:example.org
```

The access token is stored in the system keyring.
If you are on a remote machine without a keyring daemon, use the env variable `MN_NO_KEYRING`;
in this case the sync token will be stored in a file `$XDG_STATE_HOME/mn/session.json`.

### SAS Verification

Login into element (https://app.element.io), setup your account and leave it open.
Perform a login (as described above).
You should see the login in element.
Element will complain that the new login needs to be verified; start the verification from element.

```
$ mn verify
```

Compare the emojis and confirm. Done.

### Send a message

```
$ mn send -r "ROOM_ID" "Hello. :)"
```

or

```
$ echo "Hello. :)" | mn send -r "ROOM_ID"
```
