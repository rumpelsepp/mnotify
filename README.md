# mnotify

ATTENTION: Currently under development; expect breakage.

`mnotify` is a simple cli for the [matrix](https://matrix.org) chat system.
It was developed for the use case of sending notifications from a headless server.
The binary is called `mn`.
The output is always JSON (on stdout); logs go to stderr.

## Build

```
$ cargo build [--release]
```

Requires Rust 1.93 or newer (Rust edition 2024, matrix-sdk 0.18).

## Get Started

Obtain a fresh matrix user account on an arbitrary homeserver.
If you need help, checkout the matrix channel [#mnotify:hackbrettl.de](https://matrix.to/#/#mnotify:hackbrettl.de).

### Login (Password)

First, create a login.

Be aware to **always** use the complete matrix id including the domain, e.g. `@user:example.org`.
Without the `-p` flag, `mn` reads the password from stdin or interactively from the terminal.

```
$ mn login @user:example.org
```

The session (access/refresh token) and the passphrase of the encrypted state
store are kept in the system keyring. On a remote machine without a keyring
daemon set `MN_NO_KEYRING`; the secrets then live in a `0600` file
`$XDG_STATE_HOME/mnotify/$USER_ID/session.json` instead.

### Login (QR code)

Homeservers backed by a next-generation auth server (OAuth 2.0 / MAS, e.g.
matrix.org) no longer accept password logins from new clients. Log in by
scanning a QR code shown by an already signed-in device instead:

```
$ mn login @user:example.org --qr
```

`mn` asks for the base64 payload of the QR code. On a headless box, decode the
QR image you took of the other device, e.g.:

```
$ grim -g "$(slurp)" - | zbarimg --oneshot -Sbinary PNG:- | base64 -w0
```

### Verify the device / recover history

A fresh login is unverified and cannot read encrypted history. Either verify it
from another device...

```
$ mn verify
```

Start the verification from Element (or another client), compare the emojis and
confirm.

...or, if you have set up recovery before, restore the cross-signing and backup
keys from your recovery key:

```
$ mn recovery recover < recovery-key.txt
```

The first device of an account has to enable recovery once, which bootstraps
cross-signing and the server-side key backup and prints the recovery key:

```
$ mn recovery enable
{"recovery_key":"EsT ..."}
```

`mn recovery status` reports the current recovery / backup / cross-signing state.

### Send a message

```
$ mn send -r "$ROOM_ID" "Hello. :)"
```

or

```
$ echo "Hello. :)" | mn send -r "$ROOM_ID"
```

or send a file

```
$ mn send -r "$ROOM_ID" --attachment "cat.jpg"
```

### Sync

`--raw` prints the events as they come from the server.
Without `--raw` only messages are printed.

```
$ mn sync --raw
```

The sync token is persisted in the state store, so each invocation only fetches
what changed since the last one.

## Technical Stuff

### Build

Since matrix provides a lot of features, a debug build can be quite large (see
[#18](https://github.com/rumpelsepp/mnotify/issues/18)). For a smaller binary
use a `--release` build, or try
[LTO](https://doc.rust-lang.org/cargo/reference/profiles.html#lto). TLS is
always [rustls](https://github.com/rustls/rustls); there is no system-TLS
option anymore.

### Environment Variables

#### `RUST_LOG`

Standard [`tracing`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html)
filter, e.g. `RUST_LOG=matrix_sdk=debug`. Overrides `-v`/`-q`.

#### `HTTPS_PROXY`

Use this proxy for all matrix requests. Only http proxies are supported.

#### `MN_INSECURE`

Disable TLS verification.

#### `MN_NO_KEYRING`

`mnotify` uses the system keyring via the
[Secret Service API](https://specifications.freedesktop.org/secret-service/latest/).
Set this to store the secrets in a `0600` file instead. Be warned.

#### `MN_META_FILE`

Overwrite the path to `meta.json` (see below).

### Files

`mnotify` conforms to the
[XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html).

- `$XDG_STATE_HOME/mnotify/meta.json` -- which user the current session belongs to.
- `$XDG_STATE_HOME/mnotify/$USER_ID/session.json` -- session + store passphrase, only with `MN_NO_KEYRING`.
- `$XDG_STATE_HOME/mnotify/$USER_ID/store/` -- the SQLite state/crypto store, encrypted with the store passphrase.
