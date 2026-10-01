# mnotify

`mnotify` is a small command line client for [Matrix](https://matrix.org),
built for one job: **getting messages from scripts, cron jobs and headless
servers into a Matrix room**, end-to-end encrypted rooms included. It can read
too, so simple bots are a shell loop away.

The binary is called `mn`. Its output on stdout is always JSON, logs go to
stderr, and failures exit non-zero, so it composes with `jq` and `set -e`.

```
$ backup.sh || echo "Backup on $(hostname) failed" | mn send -r '#ops:example.org'
{"event_id":"$Kx0…","room_id":"!abc…:example.org"}
```

## Features

`mnotify` is deliberately not a full chat client: no UI, no calls, no room
management beyond joining. What it does support, using the categories of the
[client list on matrix.org](https://matrix.org/ecosystem/clients/):

| Feature | Status | Notes |
|---|---|---|
| End-to-end encryption | ✅ supported | Send to and read from encrypted rooms (incl. attachments), emoji verification, key backup and recovery |
| SSO | ✅ supported | Legacy `m.login.sso` (SAML, OIDC upstreams) via `--sso`, also headless over an SSH port forward |
| OAuth 2.0 / OIDC | ◐ partial | `--qr`: login by scanning a QR code with an already signed-in device (MSC4108). No browser-based OAuth flow yet |
| Threads | ◐ partial | Replies to a message in a thread stay in that thread; starting a new thread is not supported |
| Spaces | ✗ | Spaces are flagged in `mn rooms`, nothing more |
| Multiple accounts | ✗ | One account per `meta.json`; switch with `MN_META_FILE` |
| Invisible crypto (MSC4153) | ✗ | Messages are still encrypted for unverified devices |
| Sliding sync | ✅ supported | Simplified sliding sync (MSC4186) where the homeserver offers it, `/v3/sync` otherwise |
| Voice / video calls | ✗ | Out of scope |
| Custom emoji / image packs | ✗ | Out of scope |
| Multiple UI languages | ✗ | English only |

On top of that, the parts that matter for automation: JSON on stdout, room
aliases everywhere, Markdown, notices, emotes, replies, file and image
attachments with thumbnails, redactions, reading via `messages` and `sync`,
safe concurrent invocations, and secrets in the system keyring or a `0600` file.

## Showcase: alerts from a headless server in five minutes

The typical setup is a dedicated bot account that lives on the server and
posts into a room you are in.

**1. Create a bot account** on your homeserver (e.g. `@backupbot:example.org`)
with Element or your server's admin tools.

**2. Log in on the server.** A server usually has no desktop keyring; `mn`
notices that and keeps its secrets in a file readable only by you:

```
$ mn login @backupbot:example.org
password:
warning: no system keyring in use; the access token and the store passphrase are kept in plain text in ~/.local/state/mnotify/@backupbot:example.org/session.json (mode 0600). Anyone who can read this file can act as @backupbot:example.org.
```

The password can also come from stdin (`mn login … < pwfile`), which is what
you want in provisioning scripts. Avoid `-p`: it ends up in `ps` and your shell
history.

**3. Invite the bot** to your room in Element, then accept the invite on the
server. Aliases and room IDs both work everywhere:

```
$ mn join '#ops:example.org'
{"room_id":"!abc…:example.org"}
```

**4. Send.** Text as argument or on stdin, optionally as Markdown, optionally
as a notice (`m.notice`, the Matrix convention for automated messages:
clients render it less prominently and bots must not reply to it):

```
$ mn send -r '#ops:example.org' "Hello from $(hostname)"
$ df -h / | mn send -r '#ops:example.org' --notice
$ mn send -r '#ops:example.org' -n -m "**backup failed** on \`nas\`"
$ mn send -r '#ops:example.org' --attachment /var/log/backup.log
```

Set `MN_ROOM` once instead of repeating `-r`:

```
$ export MN_ROOM='#ops:example.org'
$ uptime | mn send
```

### Recipe: follow-ups in a thread of replies

`send` prints the new event ID, so later messages can reply to the first one:

```
$ id=$(mn send "Deploy of v1.4 started" | jq -r .event_id)
$ ./deploy.sh && mn send --reply-to "$id" "Deploy finished ✅"
```

### Recipe: a tiny command bot

`mn sync` runs forever and prints one JSON object per incoming event:

```sh
mn sync -r '#ops:example.org' |
  jq --unbuffered -r 'select(.type == "m.room.message" and .sender != "@backupbot:example.org")
                      | .content.body' |
  while read -r body; do
      [ "$body" = "!disk" ] && df -h / | mn send -n
  done
```

## Logging in

Always use the complete Matrix ID including the domain, e.g.
`@user:example.org`. `mn` finds the homeserver via the domain's
`.well-known/matrix/client`, falling back to `https://example.org`. If your
homeserver lives elsewhere and has no `.well-known`, name it:

```
$ mn login @user:example.org --homeserver https://matrix.example.org
```

Only one account is logged in at a time; `mn logout` ends the session on the
server and deletes all local state.

### Password

```
$ mn login @user:example.org
```

Without `-p`, `mn` reads the password interactively, or from stdin when it is
not a terminal.

### QR code (OAuth 2.0 / MAS homeservers)

Homeservers backed by a next-generation auth server (OAuth 2.0 / MAS, e.g.
matrix.org) no longer accept password logins from new clients. Instead:

```
$ mn login @user:example.org --qr
```

`mn` prints a QR code in the terminal (this works over SSH). Open an already
signed-in Element, choose "Link new device" / "Sign in with QR code", scan the
terminal, and type the two-digit check code Element shows back into `mn`.

### SSO / SAML

If your homeserver has an SSO button on the Element login page (enterprise
SAML, OIDC, ...):

```
$ mn login @user:example.org --sso
```

`mn` prints the homeserver's SSO URL and waits for the browser redirect on a
local port. Open the URL, sign in, done. On a headless box, forward the port
first (the command prints the exact `ssh -L ...` line). Use `--idp <id>` to
skip the server's identity-provider picker.

## Encrypted rooms

Sending into encrypted rooms works right after login. To also *read* encrypted
history, and so that other people's clients stop flagging the bot as
unverified, verify the device once.

Either verify it from another device of the same account...

```
$ mn verify
```

Start the verification from Element (or another client), compare the emojis and
confirm. `mn verify --device <DEVICE_ID>` starts it from this side instead.

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

## Command overview

| Command | Purpose |
|---|---|
| `mn login` / `mn logout` | Create / end the session |
| `mn join <room>` | Join a room or accept an invite |
| `mn send` | Send text, Markdown, notices, emotes, replies or files; prints the event ID |
| `mn messages -r <room>` | Print the latest messages of a room as a JSON array |
| `mn sync` | Print incoming timeline events as JSON lines, forever |
| `mn rooms` | Room details: name, members, encryption, ... |
| `mn redact` | Delete an event |
| `mn typing` | Show / hide the typing indicator |
| `mn verify`, `mn recovery` | Device verification and key backup |
| `mn whoami`, `mn homeserver` | Account and server info |
| `mn clean <user>` | Delete local state without contacting the server |

`mn <command> --help` documents every option. Every command that takes a room
accepts a room ID (`!abc:example.org`) or an alias (`#ops:example.org`).

With `-m`/`--markdown` the body is rendered as Markdown (the message keeps a
plain-text fallback for clients that don't render it). Images sent with
`--attachment` get their dimensions and, above 800px, a thumbnail, so clients
show an inline preview.

## Technical Stuff

### Build

```
$ cargo build --release
```

Requires Rust 1.93 or newer (Rust edition 2024, matrix-sdk 0.18). Use a release
build: a debug build is large (see
[#18](https://github.com/rumpelsepp/mnotify/issues/18)) and takes seconds to
open the encrypted store on every invocation. TLS is always
[rustls](https://github.com/rustls/rustls).

### Environment Variables

#### `MN_ROOM`

Default room for every command that takes `-r`/`--room`.

#### `MN_NO_KEYRING`

`mnotify` stores the session (access/refresh token) and the passphrase of the
encrypted state store in the system keyring via the
[Secret Service API](https://specifications.freedesktop.org/secret-service/latest/).
If no keyring is usable at the first login (no session bus or no Secret
Service, as on most servers), it falls back to
`$XDG_STATE_HOME/mnotify/$USER_ID/session.json` (mode `0600`) automatically,
and `mn login` prints a warning. Once that file exists it is always used. Set
`MN_NO_KEYRING` to use the file even where a keyring is available.

Anyone who can read that file can act as the account, so use a dedicated bot
account and a dedicated system user.

If you logged in where a keyring was available (e.g. on the desktop) and later
run `mn` where it is not (e.g. over SSH without a session bus), `mn` refuses to
continue rather than creating a new store passphrase. Run it from the desktop
session, or start over with `mn clean` and log in again.

#### `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`

Standard proxy variables, honoured for all Matrix requests (`socks5://`
proxies included).

#### `MN_SLIDING_SYNC`

Set to `0` during `mn login` to make that login use `/v3/sync` even where the
homeserver offers sliding sync. The choice holds until `mn logout`.

#### `MN_INSECURE`

Disable TLS verification. Only for testing.

#### `MN_META_FILE`

Overwrite the path to `meta.json` (see below).

#### `RUST_LOG`

Standard [`tracing`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html)
filter, e.g. `RUST_LOG=matrix_sdk=debug`. Overrides `-v`/`-q`.

### Files

`mnotify` conforms to the
[XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html).

- `$XDG_STATE_HOME/mnotify/meta.json` -- which user is logged in, and its homeserver URL.
- `$XDG_STATE_HOME/mnotify/$USER_ID/session.json` -- session + store passphrase, only if no keyring is used.
- `$XDG_STATE_HOME/mnotify/$USER_ID/store/` -- the SQLite state/crypto store, encrypted with the store passphrase.
