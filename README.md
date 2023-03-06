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
$ mn send -r "$ROOM_ID" "Hello. :)"
```

or

```
$ echo "Hello. :)" | mn send -r "$ROOM_ID"
```

### Sync

`--raw` prints the events as they come from the server.
Without `--raw` only messages are printed.

```
$ mn sync --raw
{"rooms":{"leave":{},"join":{},"invite":{}},"presence":{},"account_data":[],"to_device_events":[],"device_lists":{},"device_one_time_keys_count":{"signed_curve25519":50},"notifications":{}}
{"rooms":{"leave":{},"join":{},"invite":{}},"presence":{"events":[{"type":"m.presence","sender":"@rumpelsepp:hackbrettl.de","content":{"presence":"online","last_active_ago":45984,"currently_active":true}},{"type":"m.presence","sender":"@develop:hackbrettl.de","content":{"presence":"online","last_active_ago":83,"currently_active":true}}]},"account_data":[],"to_device_events":[],"device_lists":{},"device_one_time_keys_count":{"signed_curve25519":50},"notifications":{}}
```

### Technical Stuff

#### Build

Since matrix provides a lot of features, a debug build can be quite large (see [#18](https://github.com/rumpelsepp/mnotify/issues/18)).
In order to reduce the binary size, consider a `--release` build, or try [LTO](https://doc.rust-lang.org/cargo/reference/profiles.html#lto).
You can also try out the feature `native-tls` which let `mn` use the system TLS library.

#### Environment Variables

##### `HTTPS_PROXY`

Use this proxy to proxy all matrix requests.
Only http proxies are supported.

##### `MN_NO_KEYRING`

`mnotify` uses the system keyring using the [Secret Service API](https://specifications.freedesktop.org/secret-service/latest/).
If that is not desired, this variable can be set to disable the usage of the system keyring.
Instead a file `session.json` will be used for storing secrets.
I hope, you know what you're doing, be warned!

##### `MN_META_FILE`

Overwrite the path to `meta.json` (see below).

#### Files

`mnotify` conforms to the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html).

##### `$XDG_STATE_HOME/mnotify/meta.json`

Storing required meta information for the current session, such as the user.

##### `$XDG_STATE_HOME/mnotify/$USER_ID/session.json`

Used for storing secrets if `$MN_NO_KEYRING` is set.

##### `$XDG_STATE_HOME/mnotify/$USER_ID/state.$EXT`

The state store, for e.g. E2EE keys or similar.
`$EXT` is the used database system; currently `sled` is used.
However, the matrix-sdk authors are switching to `sqlite`, so this might change.
